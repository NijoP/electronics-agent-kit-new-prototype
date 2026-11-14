# EAK feature matrix (EDA desktop directive)

Status of every feature area in the owner's 2026-11-14 directive ("Transform EAK into a professional,
fully interactive EDA desktop application"), kept up to date as work lands. The design behind each row
is in `docs/archive/eda-desktop-plan/`; the EasyEDA comparison is in `docs/research/EASYEDA_FEATURE_MATRIX.md`.

**Status levels** (directive Part 17), cumulative from left to right:
`-` not started · **impl** implemented, no dedicated test found · **unit** unit tested · **integ**
integration tested (headless Rust IPC or engine end-to-end) · **desktop** real desktop tested (a real
Tauri WebView) · **eng** engineering validated (independent review) · **mfg** manufacturing validated
(fabricated and checked). "partial" means some of the feature exists but the directive's requirement is
not met; the note says what is missing. jsdom tests count as **unit**, never as **desktop**.

Baseline: `origin/main` at `afd9351`. Rows for the workspace shell, menus, tabs, themes, multilayer boards and the rule counts were corrected on 2026-11-14 from the code at `ea1efeb` (evaluator finding C6). The evidence column names the test found in the repository; nothing below was re-run for this matrix (see `TEST_RESULTS.md`). No row has reached **desktop**, **eng** or **mfg**.

**Still to re-check:** the Part 5/6 editor rows were not re-audited line by line against PR #23-#25; status levels there are as of `afd9351` unless a row says otherwise.


## Part 4: desktop shell and workspace

| Feature | Status | Evidence / note |
|---|---|---|
| Desktop app builds and starts (Tauri 2) | integ | `tauri build --no-bundle` plus a 6-second launch smoke check on Windows (PR #12); no interactive WebView test |
| Application menu bar | unit | `shell/MenuBar.tsx` (13 menus, defined in `Workspace.tsx`); `part4.test.tsx` (jsdom) |
| Main toolbar | partial (unit) | grouped toolbar in `Workspace.tsx`: open/save, undo/redo, tools, zoom, ERC, DRC, auto-route, export Gerbers, commands. There is no MECH button: `Check` is `Erc`/`Drc` only (`src-tauri/src/lib.rs`); MECH runs inside `export_manufacturing`. Save as, grid, rules and simulation are shown disabled with a reason |
| Command palette (Ctrl/Cmd+K) | unit | `App.test.tsx` |
| Document tabs | unit | `shell/DocTabs.tsx`, `shell/documents.ts`: one tab per engine document, split view, per-project restore; `part4.test.tsx` (jsdom) |
| Dirty indicator / save / save as | - | every command is durably saved; save/dirty semantics to be decided by ADR (plan Phase 1) |
| Recent projects | integ | `ipc.rs::reopen_restores_state_history_and_recent_index` |
| Dockable, resizable, persisted panels | partial (unit) | `shell/layout.ts`, `Splitter.tsx`: resizable and persisted (localStorage); no drag-to-re-dock; `part4.test.tsx` |
| Dark and light themes | unit | `design/theme.ts` (dark and light), `design/tokens.css`; `shell.test.tsx` |
| Status bar | partial | `shell/StatusBar.tsx`; `shell.test.tsx` |

## Part 5: schematic editor

| Feature | Status | Evidence / note |
|---|---|---|
| Place library part | integ | `ipc.rs::editor_flow_places_wires_routes_and_exports_gerbers`; `editors.test.tsx` (`PlaceSymbol`) |
| Wire with pin snap | integ | editor flow test; `editors.test.tsx` (`AddWire`) |
| Connect pins | integ | editor flow test; `editors.test.tsx` (`ConnectPins`) |
| Net and power labels | integ (engine) / impl (UI) | engine `LabelPin` in `eak-runtime` fixtures and `tests/engine.rs`; no UI test of the label tool found |
| Move (two-click) | impl | no `MoveSymbol` test found; no drag-move |
| Rotate (R) | integ | `ipc.rs` (`RotateSymbol`); `editors.test.tsx` |
| Delete | integ (engine) | `tests/engine.rs` (`DeleteSymbol`) |
| Edit ref/value | unit | `editors.test.tsx` (`SetSymbolValue`) |
| Mirror | - | no IR field |
| Bus, bus entry, net port, junction object | - | no IR types |
| Text and graphics | - | no IR types |
| Copy/paste, multi-select | - | |
| Drag-to-move with rubber-band wires | - | |
| Multiple sheets | partial | IR and `AddSheet` exist; sheet selector in the view; no add-sheet control |
| Annotation | impl | `Command::Annotate`; only the agent tool decode test references it |
| Net highlighting | - | |
| Undo/redo | integ | `ipc.rs::execute_undo_redo_go_through_the_engine_as_human_with_revision_checks`; engine tests |

## Part 6: PCB editor

| Feature | Status | Evidence / note |
|---|---|---|
| Board outline (rectangle, rounded rect, circle, polygon; mm/mil numeric entry, vertex table, drag handles, closure validation; arcs/circles stored as chord polylines — IR has no arc) | code-reviewed only | outline.test.ts (unrun) |
| Footprints from schematic sync | integ | `SyncSchematicToPcb` in the editor flow test |
| Move, rotate, flip footprint | unit | `editors.test.tsx` ("moves a footprint with two clicks and flips the selected one") |
| Lock/unlock | partial | engine command only, no UI |
| Track routing (45-degree / L legs, two layers) | integ | editor flow test |
| Via placement | integ | editor flow test |
| Multi-select, align, distribute, drag-move | - | |
| Multi-layer EAK-IR (more than 2 copper layers) | integ | stackup 2-16 layers, `SetLayerCount`, via spans, per-layer Gerber (PR 22); `eak-validation/tests/stackup.rs`, `eak-manufacturing/tests/multilayer.rs` |
| Layer manager | unit | layer panel and `SetLayerCount` control in the PCB view; `layers.test.ts` (jsdom). Per-layer stackup editing UI is limited |
| Interactive routing (walkaround, push-and-shove, live clearance) | - | |
| Airwires | unit | unrouted-connection airwires from the `connectivity` IPC drawn in the PCB view (`Workspace.tsx`); no dedicated test of the drawing found |
| Auto-route (two layers) | integ | `eak-manufacturing/tests/end_to_end.rs::reference_design_end_to_end` (20/20 connections) |
| Copper zones (engine, raster fill) | integ | reference design GND pour; DRC-013 zone islands |
| Copper zone editing UI, thermal reliefs | - | thermal reliefs are in unmerged PR #5 |
| Text, dimensions, keepouts, rule regions | - | |

## Part 7: component library

| Feature | Status | Evidence / note |
|---|---|---|
| Built-in library (7 generic parts) | integ | `ipc.rs::library_lists_builtin_parts_with_pins` |
| Library browser, search, filters, previews | - | |
| Per-field evidence and verified state | partial | `Rating { source, verified }` and a `provenance` string only |
| Symbol and footprint editors | - | |
| User library | - | |
| AI datasheet-to-library workflow | - | |

## Part 8: design manager and cross-probing

| Feature | Status | Evidence / note |
|---|---|---|
| Schematic <-> PCB cross-probe by component | integ | `ipc.rs::geometry_is_derived_from_eak_ir_with_cross_probe_ids`; vitest cross-probe tests (PR #12) |
| Component explorer, net explorer | - | |
| Net-level cross-probing | - | |

## Part 9: engineering verification

| Feature | Status | Evidence / note |
|---|---|---|
| ERC (13 rules) | integ | `ERC-001`..`ERC-013` declared in `erc.rs`; `eak-validation` tests, `tests/actionable_findings.rs` |
| DRC (17 rules, 2-16 layers) | integ | `DRC-001`..`DRC-017` declared in `drc.rs`; same tests; `end_to_end.rs::drc_detects_injected_short_and_clearance_violation`; `stackup.rs` for DRC-016/017 |
| MECH (3 rules) | integ | `MECH-001`..`MECH-003` in `mech.rs`; same. Run by `export_manufacturing`; no standalone UI button |
| Revision-bound reports with not-implemented lists | integ | `ipc.rs::checks_are_revision_bound_and_saved_as_evidence` |
| Findings with measured vs required values and a suggested fix | integ | ADR-0009, `actionable_findings.rs` (15 tests) |
| Click a finding -> select the component | unit | vitest cross-probe from findings (PR #12) |
| Click a finding -> navigate, zoom, highlight, safe correction | - | |
| DFM report | - | |

## Part 10: optional AI infrastructure (no assistant UI)

| Feature | Status | Evidence / note |
|---|---|---|
| Agent loop with typed tools and approvals | integ | `eak-intelligence/tests/agent.rs` (scripted provider; no live model) |
| Provider adaptor and OS-keyring keys | integ | `apps/desktop/src-tauri/tests/models.rs`, `eak-intelligence/tests/adaptor_http.rs` (loopback HTTP) |
| Domain classification panel | unit | `DomainPanel.test.tsx`; `eak-intelligence/tests/domain.rs` |
| Assistant panel (removed) | removed | The chat-style panel `shell/AiPanel.tsx` was removed in the IDE shell (#49); EAK is not an assistant UI. Agents work through the terminal workspace and the engineering harness (`docs/AGENT_HARNESS.md`). Model-provider settings, typed agent tools and approvals stay as optional infrastructure |

## Part 11: simulation and manufacturing

| Feature | Status | Evidence / note |
|---|---|---|
| Gerber, Excellon, BOM, CPL, manifest | integ | `end_to_end.rs`; editor flow export test. Gerbers not opened in an external viewer; nothing fabricated |
| Release gate (DRAFT unless the exact revision is verified) | integ | `end_to_end.rs` |
| GenCAD, IPC-2581, ODB++ | - | |
| SPICE netlist export + ngspice `.op`/`.tran` + node table + transient plot | integ | `eak-simulation` tests (real ngspice 42; skipped with a printed reason only when absent), `simulation.test.tsx`; ADR-0013. R/C/L/D/LED/V/I only, generic diode models, no AC/sweep/manufacturer models; optional runtime dependency |

## Part 13: testing levels

| Level | Status |
|---|---|
| Level 1, Rust engineering tests | in place (see `TEST_RESULTS.md`) |
| Level 2, frontend interaction tests (jsdom) | in place (43 vitest tests at PR #18) |
| Level 3, real desktop tests | **none** |

## Change log of this matrix

| Date | Change | Source |
|---|---|---|
| 2026-11-14 | Initial baseline (Phase 0) | `docs/archive/eda-desktop-plan/EDA_DESKTOP_GAP_ANALYSIS.md` |
| 2026-11-14 | Noted PRs #21 and #22 merged after the baseline, not yet reflected | `gh pr view 21`, `gh pr view 22` |
| 2026-11-15 | Phase 1 realignment: assistant row removed; terminal workspace and harness rows are tracked in `STATUS.md` (code-reviewed only). Rows above that predate the IDE shell, select/drag, grids and mirror work (e.g. Mirror, multi-select) are stale and need a re-audit | `STATUS.md`, `docs/development/TEST_EVIDENCE.md` |
