# Independent evaluation of merged modules

Evaluator: QA worker `eak-qa-e2e` (does not own any module evaluated here). Basis: a read of
`main` at `fb55118` (desktop app, `eak-runtime`, `eak-validation`, `eak-manufacturing`, tracking
docs), spot-checked by hand for findings C1, C3 and C4, plus the real-window run in
[E2E_RESULTS.md](E2E_RESULTS.md). Code reading only unless a finding says otherwise; nothing here
was fixed by the evaluator.

## What holds up

- Every visible control traced calls a real backend, or is disabled with an explicit
  "Not implemented …" reason (File › Save as, Design › Design rules, Simulation, Manufacturing ›
  Release check). No handler only logs or fakes a result.
- Frontend `IPC_COMMANDS` (`apps/desktop/src/api.ts:23`), backend `IPC_COMMANDS`
  (`src-tauri/src/lib.rs:497`) and the registered handlers (`lib.rs:655`) list the same 22
  commands; `src-tauri/tests/ipc.rs` asserts it.
- Fixture data (`fixture.test-data.ts`, `domain.test-data.ts`) is imported by tests only.
- Design state is written only through `Engine::create` / `Engine::execute`; the UI's chained
  edits each carry the revision the previous one returned.
- ERC/DRC/MECH reports carry `not_implemented` and `limitations`; no "manufacturing-ready" claim
  was found; export manifests say `DRAFT` or `INTERNALLY_VERIFIED_RELEASE_CANDIDATE`.

## Critical findings

| # | Finding | Owner area | Evidence | Why it matters |
|---|---|---|---|---|
| C1 | **DRC-008 (track current) and the relief-spoke current part of DRC-014 run only for nets with a declared current, and nothing in the desktop can declare one.** The rule is listed as run with no limitation saying undeclared nets are skipped. | `eak-validation`, desktop | `crates/eak-validation/src/drc.rs:104` (`if let Some(&amps) = c.net_currents_a.get(..)`), rule list `drc.rs:710`, limitations `drc.rs:731`; `SetNetCurrent`/`SetNetVoltage`/`UpsertNetClass`/`SetDesignRules` have no caller in `apps/desktop/src`; Design › Design rules disabled (`Workspace.tsx:465`) | Every desktop project reports DRC-008 as an implemented rule while it checks nothing; outcome can still be `NO_ERRORS_IN_IMPLEMENTED_CHECKS`. Same pattern for voltage-based clearance (`drc.rs:261,290`). Conflicts with "unknown checks never reported as passing". |
| C2 | **A package can be promoted above DRAFT while mechanical fit is UNKNOWN.** MECH-001 height check is skipped with no limit declared; a part without 3D data is only a *warning*; only errors block promotion. | `eak-validation/mech.rs`, `eak-manufacturing` | `mech.rs:14`, `mech.rs:41-56`, `eak-manufacturing/src/lib.rs:265-269` (`blocking.is_empty()` → `InternallyVerifiedReleaseCandidate`); height limit only settable via `SetDesignRules` (no UI) | Release gate treats "unknown" as passable. |
| C3 | **ERC-001's own remedy ("place a no-connect marker") has no UI.** `SetNoConnect` (and `LabelPin`) are never sent by the desktop. | desktop schematic | `crates/eak-validation/src/erc.rs:92-110`; `crates/eak-runtime/src/command.rs:96`; schematic tools `SchematicView.tsx:24-32` | Any design with an intentionally unused pin keeps an un-clearable ERC error, so a desktop-only workflow can never reach a non-DRAFT package. |
| C4 | **In split view (the default), one keypress acts in both editors.** Schematic and PCB editors each install a window-level `keydown` handler with no focus check; selection is shared. | desktop | `SchematicView.tsx:79-92`, `PcbView.tsx:151-188`, split default `Workspace.tsx:54` | `R` sends `RotateSymbol` and a footprint rotation; digit keys switch tools in both editors (4 = Label in schematic, Route in PCB). Both commands carry the same revision, so the second is expected to be rejected as stale (from code reading, not observed). |
| C5 | **Assistant-proposed edits are journaled as human edits.** Accepting a proposal calls `execute_command`, which always uses `Actor::human()`. | desktop IPC, `eak-runtime` actor policy | `Workspace.tsx:267-287`, `src-tauri/src/lib.rs:320-333`, `crates/eak-runtime/src/engine.rs:267-276` | The agent-approval path is never exercised from the desktop and the journal cannot tell AI from human changes. Lower impact today (assistant is a local rule interpreter, `shell/ai.ts:1-5`) but blocking once a model is wired in. |
| C6 | **Tracking docs are stale / unrecorded.** `FEATURE_MATRIX.md` shows `-` for shipped features (menu bar, tabs, themes, layer manager, airwires, >2 layers, zones, assistant panel); claims an ERC/DRC/**MECH** toolbar button (none exists; `Check` is `erc`/`drc` only, `lib.rs:141`); says "DRC (13 rules)" while `drc.rs:702-720` declares 17. `TEST_RESULTS.md` has nothing after the Phase 0 baseline; results for PRs #21–#26 live only in `STATUS.md`. | docs | `FEATURE_MATRIX.md:28-35,65-71,85,105`; `TEST_RESULTS.md`; `STATUS.md:229-256` | Not over-claiming, but the evidence trail is incomplete and the matrix cannot be trusted as current status. |

## Workflow step coverage (desktop)

| Step | UI control | IPC command | Gap |
|---|---|---|---|
| Create / open / reopen project | Project Home forms + Recent list | `create_project`, `open_project`, `recent_projects` | Paths typed as text (no folder picker) |
| Schematic sheet | One tab per sheet | — | `AddSheet` has no UI |
| Place components | Place tool, Parts panel, Place menu, quick open | `library_components`, `PlaceSymbol` | Built-in library only; library import has no UI |
| Wires / connect / labels | Wire, Connect pins, Label tools | `AddWire`, `ConnectPins`, `AddNetLabel` | No-connect / pin label have no UI (C3) |
| Connectivity | Airwires on PCB, unrouted count in Layers panel | `connectivity` | No netlist / net-highlight view |
| Schematic → PCB | Project menu, palette | `SyncSchematicToPcb` | Not on a toolbar |
| Footprints & layers | Move/rotate/flip, Auto-place, layer count, Layer manager | `MoveFootprint`, `AutoPlace`, `SetLayerCount`, `SetStackup` | No footprint chooser; lock has no UI |
| Routing & zones | Route, Via, Zone tools; Route menu auto-route/clear | `AddTrack`, `AddVia`, `AddZone`, `AutoRoute`, `ClearRouting` | Auto-router outer 2 layers only (stated limitation) |
| Design rules / net classes | Disabled ("Not implemented") | none from UI | C1, C2 |
| ERC / DRC | Toolbar, Design menu, findings panel | `run_check` | — |
| Manufacturing (MECH) check | none on its own | only inside `export_manufacturing` | C6 (matrix claims a button) |
| Outputs | Toolbar Export Gerbers, Manufacturing panel | `export_manufacturing` | Output folder fixed to the project |
| Save / close | Ctrl+S message (engine saves every command), Close | `close_project` | Save as disabled with reason |

## Lower-severity notes

- `model_set_key` is registered and wrapped (`api.ts:73`) but never called; keys go through
  `model_save_provider`.
- The status bar shows the configured model name although chat does not use a model yet
  (`StatusBar.tsx:32`; only the assistant panel says so).
- The bottom-panel BOM is recomputed in the frontend (`BottomPanel.tsx:29-39`) instead of using
  `eak_manufacturing::bom_csv`; it can drift from the exported BOM.
- `eak_runtime::store::write_project` is `pub` (`store.rs:129`); only the engine calls it today.
