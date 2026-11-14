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
| Engineering calculators, router, placer | build/test-verified | IPC tables not checked against the standard text |
| Gerber / drill / BOM / CPL, JLCPCB preparation | build/test-verified | DRAFT output; no external viewer or fabricator check |
| Schematic and PCB editors (select, drag, flip, grids, align) | build/test-verified (jsdom + engine) | not exercised in a real window |
| IDE shell: workspaces, explorer, multi-project, Project Home | build/test-verified at #49 | merged before the freeze; real-window run not done |
| Install scripts and v0.1.4 Linux release | code-reviewed only (scripts); package published | no install of the published package has been verified on real hardware (`docs/PLATFORM_SUPPORT.md`); Windows/macOS: no published binaries |
| Terminal engine (`eak-terminal`) | runtime-verified (real-PTY tests passed before the override, Linux) | later edits and the derived agent table test: not run |
| Terminal workspace UI, registry wiring | code-reviewed only | |
| Engineering harness (`eak` CLI, MCP, app endpoint) | code-reviewed only | never compiled; e2e tests written, never run |
| Provider catalog, streaming, Gemini, staged connection test | code-reviewed only | |
| Integration of all of the above (this commit) | code-reviewed only | conflicts resolved by reading; `Cargo.lock` hand-merged |
| Phase 2 wiring: harness env in terminal sessions, space-project binding, editor resync after checks/exports | code-reviewed only | `git diff --check` and grep only; never compiled or run |
| Engineering gates (PASS/FAIL/UNKNOWN/NOT_RUN), copper/power engineering, physical quantities, harness `run_gates` | code-reviewed only | ADR-0021; never compiled or run |
| Autonomous design flow (`eak design`, `design_*` tools, evidence labels, TTY-only human approval; ADR-0020) | code-reviewed only | DESK mental models / 12-step procedure / derating (`eak-intelligence::mental`), domain gates blocking apply, intent read-back, component evidence; never compiled or run; unit tests in `design.rs` and `calc.rs` written, unrun; worked example in `docs/agents/AUTONOMOUS_DESIGN.md` not run |

## Known gaps
- No cargo, npm, vitest, tsc or tauri run has happened on the integrated tree.
- Harness env is exported to terminal sessions (code-reviewed only, never run).
- Autonomous design loop exists as harness tools only (code-reviewed only, never run); no per-call approval UI for agent edits; no Windows named pipes.
- No agent (Claude Code, OpenCode, Pi) has been tested end to end through the harness.
- 3D board viewer (three.js, registered EDA view, read-only: extruded outline, copper, pads, tracks, vias, courtyard outlines, bodies from library body height, orbit/presets/layer toggles): written, **code-reviewed only** (never built or run). Differential pairs, impedance control, requirements capture: not implemented.
- `FEATURE_MATRIX.md` rows for older features need a re-audit.
- Fail-closed engineering gates, unit-aware copper/power calculations, typed physical quantities (ADR-0021): **code-reviewed only**. IPC tables remain unverified against the standard text; reference nets are recognised by name.
