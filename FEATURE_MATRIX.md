| Footprints from schematic sync | integ | `SyncSchematicToPcb` in the editor flow test |
| ECO preview before Update PCB (slice C2, ADR-0041) | partial (CI: `tests/eco.rs` property test, vitest, ui-evidence `c2-*`) | read-only `sync_preview` change list reviewed in a dialog (group by action/object, per-change checkboxes, export report); apply = one `Batch` at the previewed revision, stale revision rejected. Applying only some changes is not available (Apply disabled with the reason while a change is unchecked; needs a selective-sync command, slice C4). No back-annotation |# EAK feature matrix (EDA desktop directive)

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
| Application menu bar | unit (code-reviewed only for slice A1, CI pending) | Slice A1 (2026-11-23): generated per document from the action registry `src/actions/` (`registry.ts`, `sets.ts`, `types.ts`); schematic shows Place (part, wire, label, no connect, pin label) and Simulation, PCB shows Place (via, outline, zone), Route and Manufacturing, the 3D view has no editing menus; menu Place/Route entries arm the same editor tool as the toolbar. Tests: `actions/registry.test.ts`, `part4.test.tsx` (jsdom). Real-window screenshots: pending (CI Xvfb job) |
| Main toolbar | partial (unit; slice A2 code-reviewed only, CI pending): file/history, edit, checks, view, manufacturing, More menu when narrow | grouped toolbar generated from the action registry (`src/actions/`, slice A1; every toolbar action also has a menu entry): open/save, undo/redo, tools, zoom, ERC, DRC, auto-route, export Gerbers, commands. Slice A2 (2026-11-23): every button shows an EAK icon (`src/design/icons/`, 83 icons, audit in its README) with label, tooltip and shortcut; View > Toolbar labels shows text too; split buttons (net marks, board regions, PCB checks) remember the last pick; the View group holds the active editor's grid size, unit (mm/mil/inch) and snap combos, which drive that editor's snapping, rulers and cursor readout (View menu: Show grid, Snap to grid, Finer/Coarser grid, Unit). There is no MECH button: `Check` is `Erc`/`Drc` only (`src-tauri/src/lib.rs`); MECH runs inside `export_manufacturing`. Save as and Junction were removed in slice E1 (A-SAVE: the engine writes every accepted command; A-JUNCTION: junction dots are derived); no toolbar button is a permanently disabled placeholder. Tests: `shell/toolbar.fold.test.tsx`, `toolbar.test.tsx`, `actions/registry.test.ts`, `grid.test.ts`, `design/icons/icons.test.ts`, `tools/pcb/PcbSelect.test.tsx`, `tools/schematic/twoResistors.test.tsx` (jsdom). Real-window screenshots: `a2-*` steps of the CI ui-evidence scenario |
| UI inventory gate (every visible control has a handler or is unavailable with a reason) | partial (unit, CI only; slice E9) | `src/qa/uiInventory.test.ts` generates `docs/qa/UI_INVENTORY.md` from the action registry and a static JSX scan and fails on unallowlisted violations (allowlist `src/qa/uiInventoryAllowlist.ts`, owners named). Static: a handler that does nothing is not detected. Real-window: `e9-*` steps of the CI ui-evidence scenario |
| Command palette (Ctrl/Cmd+K) | unit | `App.test.tsx` |
| Document tabs | unit | `shell/DocTabs.tsx`, `shell/documents.ts`: one tab per engine document, split view, per-project restore; per-document marker (slice E1): the active document while its command is in flight, every engine document while writes are blocked, never a file viewer; closing asks only for a marked document; `part4.test.tsx`, `deadControls.test.tsx` (jsdom) |
| Dirty indicator / save / save as | unit (slice E1, code-reviewed only, CI pending) | A-SAVE applied: every accepted command is durably saved, so there is no unsaved state. File > Save (confirm written) asks the engine and reports blocked writes; Save as is removed (no copy-the-project feature yet); the toolbar badge shows Saved / Writing… / Writes blocked from the engine's busy and `blocked` state. Tests: `deadControls.test.tsx` (jsdom); real-window step `e1-saved` |
| Recent projects | integ | `ipc.rs::reopen_restores_state_history_and_recent_index`; Project Home search/table/shortcuts code-reviewed only (`projectHome.test.ts` written, unrun) |
| Dockable, resizable, persisted panels | partial (unit; slice A3: CI vitest and real-window UI evidence rows a3-*) | Slice A3 (2026-11-23): three docks (left, right, bottom), each with tabs, hide/restore (View menu, Ctrl+B / Ctrl+Shift+B / Ctrl+J, a hide button) and a resizable splitter (drag, arrow keys, double-click resets); open state, size and active tab saved per workspace (home, schematic, PCB, 3D, file) in `shell/layout.ts` (pre-A3 layout migrated). No drag-to-re-dock between docks. Tests: `shell/docks.test.ts`, `docks.app.test.tsx`, `part4.test.tsx` (jsdom). Real-window: `a3-*` steps of the CI ui-evidence scenario |
| Design manager (left dock) | unit (slice A3: CI vitest and real-window UI evidence rows a3-*) | `shell/DesignManager.tsx`, `shell/designManagerData.ts`: Pages / Nets / Components / Objects with search, from the snapshot, the engine netlist (`netlist` IPC) and connectivity (`connectivity` IPC); a click opens the page, or selects and zooms to the part, net or object (PCB: highlight + zoom; schematic: select + zoom). No net selection model in the schematic yet (a net zooms there but is not highlighted). Tests: `shell/docks.test.ts`, `docks.app.test.tsx` |
| Right dock: Properties / Filter / Layers | partial (slices A3, A5: CI vitest and real-window UI evidence rows a5-*) | `shell/RightDock.tsx`: Properties is the A5 context inspector (`shell/Inspector.tsx`, `shell/inspectorModel.ts`): the active tool's options, else the selection's fields (common fields for several objects), else the document (sheet size; board, copper-layer count, stack-up); every edit is one typed command (a `Batch` for several objects: one revision, one undo step); fields the EAK-IR cannot hold yet (title block, wire style, MPN/supplier, BOM flags, zone fill, layer materials) are shown unavailable with the reason (slice B1). Tests: `inspector.test.tsx` (jsdom, per mode); Filter is shown disabled with its reason until slice B3; Layers (moved from the activity bar) in the PCB only. The bottom dock gained the Library tab, redesigned as the Library dock by slice D1 (Part 7) |
| Canvas strip | unit (slice A3: CI vitest and real-window UI evidence rows a3-*) | `shell/CanvasStrip.tsx` under the active editor: pointer X/Y (design coordinates, snapped when snap is on), dX/dY from the last press, zoom (100 % = actual size at 96 dpi), grid, snap and unit from the A2 grid store; read-only (the toolbar holds the controls). Tests: `shell/docks.test.ts`, `docks.app.test.tsx` |
| Dark and light themes | unit | `design/theme.ts` (dark and light), `design/tokens.css` (full token scale, `docs/design-system.md`; guard `design/tokens.test.ts` written, unrun); `shell.test.tsx` |
| Project overview (side panel) | code-reviewed only | `shell/ProjectOverview.tsx`: real snapshot, check, connectivity, domain, event and terminal data; `shell/projectOverview.test.ts` written, unrun |
| Status bar | partial | `shell/StatusBar.tsx`: copper layer only while the PCB editor is active, grid of the active editor (slice E1); `shell.test.tsx`, `deadControls.test.tsx` |
| Settings, hotkeys and canvas colours (Phase 1 slice D4) | unit (code-reviewed only, CI pending) | Settings > Settings… (`src/settings/SettingsDialog.tsx`): searchable tree of pages. Hotkeys: table generated from the action registry, search by command or by key press, rebind by key press, remove, reset; a key already used by an action in a shared document, or owned by an editor (tool digits, R, W, Delete…), is refused with the reason; presets EAK default and an EAK-authored EasyEDA-style map (`settings/keymap.ts`, not a copy of a vendor preset); rebindings persist per machine and show in menus, toolbar tooltips, the palette, Help and the key handler. Canvas colours: per-object table over the `--c-*` tokens with a printable Paper preset (`settings/canvasColours.ts`; copper layer colours stay in the Layers panel). Schematic and PCB grid defaults (spacing, unit, grid, snap) write each editor's grid store. Tests: `settings/settings.test.tsx`; real-window steps `apps/desktop/e2e/scenarios/d4-settings.json` |
| Help and About | unit (slice E1, code-reviewed only, CI pending) | Help > Keyboard shortcuts is generated from the action registry of the active document plus the active editor's tool list (`actions/editorKeys.ts`); Help > About opens a dialog with the app version (`src/version.ts`, checked against `tauri.conf.json`); both close on Escape. Tests: `deadControls.test.tsx`, `version.test.ts`; real-window steps `e1-help`, `e1-about` |

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
| Bus, bus entry, net port (scope local / global / off-page), power-port style | engine (slice B1, code-reviewed only, CI pending); no UI | IR schema v3 (ADR-0040): `Bus`, `BusEntry`, `NetLabel.scope`, `NetLabel.power_style`; commands `AddBus`/`MoveBus`/`DeleteBus`, `AddBusEntry`/`MoveBusEntry`/`DeleteBusEntry`, `SetNetLabelScope`, `SetPowerPortStyle`; netlist honours scope and bus members, ERC-017..021. Tests: `eak-runtime/tests/ir_v3.rs`, `eak-validation/tests/bus_ports.rs`, netlist unit tests. Power ports are drawn as ground, rail or chassis shapes (slice B2, `powerPortShape`); buses, bus entries and port scopes are not drawn yet (slices B3-B6). Junction objects: none (junction dots are derived) |
| Text and graphics (sheet and symbol), drawing format and title block, field visibility | engine (slice B1, code-reviewed only, CI pending); no UI | `Graphic` (line, polyline, arc, circle, rectangle, polygon, text) in `SymbolDef.graphics` and `Schematic.graphics`; `Sheet.format` (border, zones, title-block fields bound to project properties); `SymbolInstance.field_visibility`; commands `AddSheetGraphic`/`MoveSheetGraphic`/`DeleteSheetGraphic`, `SetSheetFormat`, `SetProjectProperty`, `SetFieldVisibility`. Slice B2: the schematic view draws symbol graphics, pin names and numbers (`render/symbolDraw.ts`, `tools/schematic/SymbolSvg.tsx`), built-in parts have real drawings and the KiCad importer maps symbol graphics; sheet graphics, drawing format and field visibility are not drawn yet |
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
| Interactive routing (walkaround, push-and-shove, live clearance) | code-reviewed only | walkaround and live clearance in `eak_engineering::interactive`; manual workflow in `routeTool.ts`/`PcbView.tsx` (pad hit, width, via+layer, track edit/delete, next-connection priority); slice C3 routing options in the inspector: width and via presets from the net classes, 45/90 corners (arc unavailable), obstacle mode walk around / push / block, loop removal (one `Batch`), ratsnest toggle, live net length; push-and-shove (single level: other-net tracks bumped aside keeping their ends, pads/vias never move, moved tracks committed in the route's `Batch`, ADR-0042). Tests: `routeTool.test.ts` (C3 presets, loop removal, net length, push pending/Batch), `interactive.rs` (block mode, via preset, push), `eak-validation/tests/push_shove.rs` (no DRC-002/003 after push), CI ui-evidence `c3-*` |
| Editor correctness and atomicity (Phase 1 slice E2) | unit + engine (code-reviewed only, CI pending) | one `Batch` per user action for L-wire, fill planes and net declaration; route vias with inner-layer span; track layer change keeps connectivity (vias) or refuses; no-connect per sheet, deduplicated, removable (`DeleteNoConnect`, ADR-0038); Place click on a symbol does not place; PCB R/F scoped to Select/Move; layer count 2..16; Esc cancels vertex drag. Tests: `routeTool.test.ts`, `layers.test.ts`, `rules.test.tsx`, `editors.test.tsx`, `tools/schematic/editorCorrectness.test.tsx`, `tools/schematic/dragPlacement.test.tsx`, `tools/pcb/PcbSelect.test.tsx` (jsdom); `eak-runtime/tests/schematic_edit.rs`. Real-window: `e2-*` steps of the CI ui-evidence run |
| Airwires | unit | unrouted-connection airwires from the `connectivity` IPC drawn in the PCB view (`Workspace.tsx`); no dedicated test of the drawing found |
| PCB rendering (Canvas2D scene renderer, slice A4) | integ (CI, Chrome under Xvfb) | `src/render/` (ADR-0037); default renderer where a 2D context exists, SVG fallback behind the PCB renderer selector. Tests: `render/render.test.ts`, `render/parity.test.tsx`; CI job `render-qa` (pixel-diff snapshots of 3 reference boards, real-click picking, screenshots, benchmark). 30 fps not met for whole-board zoom or sub-pixel pan at 2,000 footprints / 20,000 segments; Tauri WebKitGTK and reference host not measured |
| Auto-route (two layers) | integ | `eak-manufacturing/tests/end_to_end.rs::reference_design_end_to_end` (20/20 connections); connection order is now net priority (power/ground, high current, terminal count; DESK model in `eak_engineering::priority`) then length; plus congestion cost, multi-pass with stagnation stop, selective rip-up, diff-pair adjacency, current-sized widths (`routing_intel`, ADR-0023) - code-reviewed only, unrun |
| Existing-project rework (inspect, baseline, audit, protect, plan, human approval, apply with readback, report) | code-reviewed only | `eak-harness/src/rework.rs`, `docs/agents/REWORK.md`, ADR-0022; unit tests written, unrun; no end-to-end run on a real project |
| Copper zones (engine, raster fill) | integ | reference design GND pour; DRC-013 zone islands |
| Zone stitching (`StitchZones`), dead-copper removal | code-reviewed only | DESK `stitch_orphans` method, ADR-0030; never compiled or run |
| Copper zone editing UI, thermal reliefs | - | thermal reliefs are in unmerged PR #5 |
| Text, dimensions, keepouts, rule regions | - | |

## Part 7: component library

| Feature | Status | Evidence / note |
|---|---|---|
| Built-in library (7 generic parts) | integ | `ipc.rs::library_lists_builtin_parts_with_pins` |
| Library browser, search, filters, previews | impl (slice D1, code-reviewed only, CI pending) | Library dock in the bottom dock: kinds Part / Symbol / Footprint / 3D, scopes built-in / project / personal / imported / distributor, category tree, paged grid with supplier, stock and price (ADR-0028), symbol / footprint / generated 3D-body preview, Place and drag-to-place, offline mode (built-in and project only); `shell/libraryDockView.test.tsx`, ui-evidence `d1-library.json` |
| Common parts palette (generic R/C/L/D/Q with package variants) | impl (slice D1, code-reviewed only) | `shell/libraryDockModel.ts` `COMMON_PARTS`, only ids the built-in library lists; `shell/libraryDockModel.test.ts` |
| Part standardisation view (SOURCING check over placed parts and stored records) | impl (slice D1, code-reviewed only) | IPC `part_standardisation` (`parts.rs`); `parts_ipc.rs::part_standardisation_runs_the_sourcing_check_without_writing`; lists `not_implemented` and `limitations`; never says parts are orderable |
| Per-field evidence and verified state | partial | `Rating { source, verified }` and a `provenance` string only |
| Symbol and footprint editors | - | |
| User library | - | |
| Component 3D model reference (STEP/WRL/GLB, transform, quality, licence), viewer loading, missing-model state | impl | code-reviewed only, unrun (ADR-0025); no model-vs-footprint verification or collision checks |
| Supplier offers separate from component identity (`SupplierOffer`, legacy `distributor` migration), resilient Distributor search/import (cache, retry, stale label, typed errors) | impl | code-reviewed only, unrun (ADR-0028); no lifecycle parsing, cache not persisted, offer data never used as suitability evidence |
| Distributor catalogue through the documented signed OpenAPI (credentials in OS keyring, test connection, category/manufacturer browsing, paged search, exact/normalised/fuzzy explainable matching, live/cached/stale markers) | impl | code-reviewed only, unrun (ADR-0032); never run against a live Distributor account; response field mapping inferred from public docs; package/manufacturer filters are client-side per page |
| Library scopes (built-in/project/personal/imported open-source with licence), cross-scope browser (filters, favourites, tags, recents, pin/pad inspection), supplier discovery separate from engineering import, listing standing (existing / supplier-only / needs authoring) | impl | code-reviewed only, unrun (ADR-0033); only EAK ComponentDef JSON is imported (no external format translation, no authoring wizard, no bulk matching); metadata per machine |
| Engine-owned sourcing records; KiCad licence gate; enabled flag and cross-scope placement (Phase 1 slice E3) | unit + integ (code-reviewed only, CI pending) | 2026-11-23: supplier/assembly records through `Engine::write_project_record` (ADR-0039; `eak-runtime/tests/project_records.rs`, `parts_ipc.rs::assembly_attestations_are_written_through_the_engine_and_journalled`, CI `scripts/check-engine-boundary.sh`); KiCad commit refused without licence and http(s) source (`kicad.rs` unit tests, `kicadImport.test.tsx`); Library panel search and Place tool list honour the enable switch and list personal/imported parts, placement copies the part into the project in one Batch (`library_ipc.rs::placing_a_personal_or_imported_part_copies_it_into_the_project_in_one_batch`, `library_ipc.rs::library_search_and_place_list_honour_the_enabled_flag_and_show_every_scope`, `libraryPanel.test.tsx`, `dragPlacement.test.tsx`). Real-window: `e3-*` steps of the ui-evidence workflow |
| Fabricator assembly classification separate from Distributor listing (attested or parts-file only), fail-closed BOM warnings, prep manifest assembly report | impl | code-reviewed only, unrun (ADR-0034); no live Fabricator lookup; warnings do not block |
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
| Fail-closed engineering gates (`run_gates`, release blocking) | code-reviewed only | `eak-validation/src/gates.rs`, ADR-0021; tests written, never run |
| Multi-domain engineering model (`intel_*`, `intelligence_summary`) | code-reviewed only | `eak-intelligence/src/systems/`, ADR-0028; tests written, never run |
| Copper/power engineering (explicit-input trace width, voltage drop, rail drop, return path) | code-reviewed only | `eak-engineering/src/power.rs`; tests written, never run |

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
| 2026-11-15 | Autonomous design flow (`eak design`, `design_*` harness tools, ADR-0020): code-reviewed only, never compiled or run; tracked in `STATUS.md` | `docs/agents/AUTONOMOUS_DESIGN.md` |
| 2026-11-15 | Knowledge retrieval (BM25, provenance, advisory; ADR-0024): code-reviewed only, unit tests written and unrun (`eak-intelligence/src/knowledge.rs`, `eak-harness/src/knowledge.rs`; code-reviewed only) |
| 2026-11-15 | Performance directive: resource profile, bounded jobs, lock-free analysis, pointer coalescing, 3D context-loss/quality tier (code-reviewed only, nothing measured); tracked in `STATUS.md` | `docs/development/PERFORMANCE_LEDGER.md`, `docs/development/PERFORMANCE.md` |
| 2026-11-15 | Live EDA synchronisation (harness commit → typed `eak://revision-changed` event → one snapshot refetch, stale events rejected, derived views refetch only changed areas; ADR-0031) and CI enabled: code-reviewed only; tests run in GitHub Actions only | `docs/AGENT_HARNESS.md`, `.github/workflows/ci.yml` |
| 2026-11-15 | CI enabled and green on Linux, Windows and macOS (arm64 + x64): fmt, clippy `-D warnings`, `cargo test --workspace --locked`, frontend typecheck + 253 tests ([run 43733291036](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/43733291036), `b62d0e2`; [run 43734737480](https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/43734737480), `b096688`). Rows above marked "code-reviewed only" are now compiled and test-passing in CI but still not run in a real window or on hardware | `.github/workflows/ci.yml` |
| 2026-11-16 | Library identity layer (`eak-core/src/libmeta.rs`: library manifest, scopes, maturity with evidence rules, collision-rejecting catalogue, committed-definition references with drift detection; ADR-0035), KiCad and Weft audits, vendor-neutral architecture, third-party reuse review: static-review only, test source added, not runtime verified; nothing wired to desktop yet |
- 2026-11-16: agent task phases, PCB-sync ERC gate and shared revision/task status (ADR-0036): code-reviewed only, CI pending.
| 2026-11-16 | Bounded S-expression reader (`eak-core/src/sexpr.rs`: size/depth/node limits, line-numbered errors, no execution) as the safe base for KiCad symbol/footprint import (Milestone 3 groundwork; no importer yet): static-review only, test source added, not runtime verified |
| 2026-11-16 | Optional one-way "Import from KiCad" converter (KiCad text files into EAK-native definitions; EAK never needs KiCad, its libraries or its formats at runtime) (`eak-core/src/kicad_import.rs`: pin/pad mapping, duplicate rejection, counted unsupported elements, per-record notes), pin/pad mapping check (`pinmap.rs`) and a SyncSchematicToPcb gate that refuses to stage a footprint with a blocking mapping and reports it: static-review only, test source added, not runtime verified; not yet exposed in the desktop |
| 2026-11-16 | Library catalogue on the desktop (`libcatalog.rs`: built-in, project, personal and imported libraries as libmeta manifests with version, licence, source, content hash; typed IPC `library_catalog` / `library_set_enabled` registered in the command list; disabled libraries hidden from search and placement via `libstore::all_defs`; `LibraryManager.tsx` mounted in the library panel): static-review only, unit test source for the pure helpers added, no frontend test yet, not runtime verified |
