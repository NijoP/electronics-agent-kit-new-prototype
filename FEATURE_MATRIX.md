# EAK feature matrix (EDA desktop directive)

Status of every feature area in the owner's 2026-11-14 directive ("Transform EAK into a professional,
fully interactive EDA desktop application"), kept up to date as work lands. The design behind each row
is in `docs/architecture/`; the EasyEDA comparison is in `docs/research/EASYEDA_FEATURE_MATRIX.md`.

**Status levels** (directive Part 17), cumulative from left to right:
`-` not started · **impl** implemented, no dedicated test found · **unit** unit tested · **integ**
integration tested (headless Rust IPC or engine end-to-end) · **desktop** real desktop tested (a real
Tauri WebView) · **eng** engineering validated (independent review) · **mfg** manufacturing validated
(fabricated and checked). "partial" means some of the feature exists but the directive's requirement is
not met; the note says what is missing. jsdom tests count as **unit**, never as **desktop**.

Baseline: `origin/main` at `afd9351` (2026-11-14). The evidence column names the test found in the
repository; nothing below was re-run for this matrix (see `TEST_RESULTS.md`). No row has reached
**desktop**, **eng** or **mfg**.

**Not yet reflected:** [PR #21](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/21)
(workspace shell) and [PR #22](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/22)
(multilayer boards, 2-16 layers, via spans) merged after the `afd9351` baseline (`main` at `6a3b358`).
The Part 4 and Part 6 rows below predate them and must be re-checked against their tests before any
status is raised.

## Part 4: desktop shell and workspace

| Feature | Status | Evidence / note |
|---|---|---|
| Desktop app builds and starts (Tauri 2) | integ | `tauri build --no-bundle` plus a 6-second launch smoke check on Windows (PR #12); no interactive WebView test |
| Application menu bar | - | |
| Main toolbar | partial (unit) | undo/redo, ERC/DRC/MECH, export, layout toggle; no save-as, grid, rules or simulation |
| Command palette (Ctrl/Cmd+K) | unit | `App.test.tsx` |
| Document tabs | - | layout toggle only |
| Dirty indicator / save / save as | - | every command is durably saved; save/dirty semantics to be decided by ADR (plan Phase 1) |
| Recent projects | integ | `ipc.rs::reopen_restores_state_history_and_recent_index` |
| Dockable, resizable, persisted panels | - | |
| Dark and light themes | - | one dark theme |
| Status bar | partial | revision and saved state shown in the header |

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
| Board outline (rectangle, polygon) | integ | editor flow test |
| Footprints from schematic sync | integ | `SyncSchematicToPcb` in the editor flow test |
| Move, rotate, flip footprint | unit | `editors.test.tsx` ("moves a footprint with two clicks and flips the selected one") |
| Lock/unlock | partial | engine command only, no UI |
| Track routing (45-degree / L legs, two layers) | integ | editor flow test |
| Via placement | integ | editor flow test |
| Multi-select, align, distribute, drag-move | - | |
| Multi-layer EAK-IR (more than 2 copper layers) | - | `CopperLayer` is Top/Bottom; PR #4 (per-layer stack-up, schema v2) unmerged |
| Layer manager | - | |
| Interactive routing (walkaround, push-and-shove, live clearance) | - | |
| Airwires | - | |
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
| ERC (13 rules) | integ | `eak-validation` tests, `tests/actionable_findings.rs` |
| DRC (13 rules, two layers) | integ | same; `end_to_end.rs::drc_detects_injected_short_and_clearance_violation` |
| MECH (3 rules) | integ | same |
| Revision-bound reports with not-implemented lists | integ | `ipc.rs::checks_are_revision_bound_and_saved_as_evidence` |
| Findings with measured vs required values and a suggested fix | integ | ADR-0009, `actionable_findings.rs` (15 tests) |
| Click a finding -> select the component | unit | vitest cross-probe from findings (PR #12) |
| Click a finding -> navigate, zoom, highlight, safe correction | - | |
| DFM report | - | |

## Part 10: AI-native workflow

| Feature | Status | Evidence / note |
|---|---|---|
| Agent loop with typed tools and approvals | integ | `eak-intelligence/tests/agent.rs` (scripted provider; no live model) |
| Provider adaptor and OS-keyring keys | integ | `apps/desktop/src-tauri/tests/models.rs`, `eak-intelligence/tests/adaptor_http.rs` (loopback HTTP) |
| Domain classification panel | unit | `DomainPanel.test.tsx`; `eak-intelligence/tests/domain.rs` |
| Assistant panel, agent activity panel, proposal preview UI | - | |

## Part 11: simulation and manufacturing

| Feature | Status | Evidence / note |
|---|---|---|
| Gerber, Excellon, BOM, CPL, manifest | integ | `end_to_end.rs`; editor flow export test. Gerbers not opened in an external viewer; nothing fabricated |
| Release gate (DRAFT unless the exact revision is verified) | integ | `end_to_end.rs` |
| GenCAD, IPC-2581, ODB++ | - | |
| SPICE netlist, simulator, waveform viewer | - | |

## Part 13: testing levels

| Level | Status |
|---|---|
| Level 1, Rust engineering tests | in place (see `TEST_RESULTS.md`) |
| Level 2, frontend interaction tests (jsdom) | in place (43 vitest tests at PR #18) |
| Level 3, real desktop tests | **none** |

## Change log of this matrix

| Date | Change | Source |
|---|---|---|
| 2026-11-14 | Initial baseline (Phase 0) | `docs/architecture/EDA_DESKTOP_GAP_ANALYSIS.md` |
| 2026-11-14 | Noted PRs #21 and #22 merged after the baseline, not yet reflected | `gh pr view 21`, `gh pr view 22` |
