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
| Application menu bar | unit (code-reviewed only for slice A1, CI pending) | Slice A1 (2026-11-23): generated per document from the action registry `src/actions/` (`registry.ts`, `sets.ts`, `types.ts`); schematic shows Place (part, wire, label, no connect, pin label) and Simulation, PCB shows Place (via, outline, zone), Route and Manufacturing, the 3D view has no editing menus; menu Place/Route entries arm the same editor tool as the toolbar. Tests: `actions/registry.test.ts`, `part4.test.tsx` (jsdom). Real-window screenshots: pending (CI Xvfb job) |
| Main toolbar | partial (unit); regrouped 2026-11-15 (code-reviewed only, unrun): file/history, edit, checks, view, manufacturing, More menu when narrow | grouped toolbar generated from the action registry (`src/actions/`, slice A1; every toolbar action also has a menu entry): open/save, undo/redo, tools, zoom, ERC, DRC, auto-route, export Gerbers, commands. There is no MECH button: `Check` is `Erc`/`Drc` only (`src-tauri/src/lib.rs`); MECH runs inside `export_manufacturing`. Save as, grid, rules and simulation are shown disabled with a reason |
| Command palette (Ctrl/Cmd+K) | unit | `App.test.tsx` |
| Document tabs | unit | `shell/DocTabs.tsx`, `shell/documents.ts`: one tab per engine document, split view, per-project restore; `part4.test.tsx` (jsdom) |
| Dirty indicator / save / save as | - | every command is durably saved; save/dirty semantics to be decided by ADR (plan Phase 1) |
| Recent projects | integ | `ipc.rs::reopen_restores_state_history_and_recent_index`; Project Home search/table/shortcuts code-reviewed only (`projectHome.test.ts` written, unrun) |
| Dockable, resizable, persisted panels | partial (unit) | `shell/layout.ts`, `Splitter.tsx`: resizable and persisted (localStorage); no drag-to-re-dock; `part4.test.tsx` |
| Dark and light themes | unit | `design/theme.ts` (dark and light), `design/tokens.css` (full token scale, `docs/design-system.md`; guard `design/tokens.test.ts` written, unrun); `shell.test.tsx` |
| Project overview (side panel) | code-reviewed only | `shell/ProjectOverview.tsx`: real snapshot, check, connectivity, domain, event and terminal data; `shell/projectOverview.test.ts` written, unrun |
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
| Interactive routing (walkaround, push-and-shove, live clearance) | code-reviewed only | walkaround and live clearance in `eak_engineering::interactive`; manual workflow in `routeTool.ts`/`PcbView.tsx` (pad hit, width, via+layer, track edit/delete, next-connection priority); push-and-shove not implemented; new tests in `routeTool.test.ts` are unrun |
| Airwires | unit | unrouted-connection airwires from the `connectivity` IPC drawn in the PCB view (`Workspace.tsx`); no dedicated test of the drawing found |
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
| Library browser, search, filters, previews | - | |
| Per-field evidence and verified state | partial | `Rating { source, verified }` and a `provenance` string only |
| Symbol and footprint editors | - | |
| User library | - | |
| Component 3D model reference (STEP/WRL/GLB, transform, quality, licence), viewer loading, missing-model state | impl | code-reviewed only, unrun (ADR-0025); no model-vs-footprint verification or collision checks |
| Supplier offers separate from component identity (`SupplierOffer`, legacy `distributor` migration), resilient Distributor search/import (cache, retry, stale label, typed errors) | impl | code-reviewed only, unrun (ADR-0028); no lifecycle parsing, cache not persisted, offer data never used as suitability evidence |
| Distributor catalogue through the documented signed OpenAPI (credentials in OS keyring, test connection, category/manufacturer browsing, paged search, exact/normalised/fuzzy explainable matching, live/cached/stale markers) | impl | code-reviewed only, unrun (ADR-0032); never run against a live Distributor account; response field mapping inferred from public docs; package/manufacturer filters are client-side per page |
| Library scopes (built-in/project/personal/imported open-source with licence), cross-scope browser (filters, favourites, tags, recents, pin/pad inspection), supplier discovery separate from engineering import, listing standing (existing / supplier-only / needs authoring) | impl | code-reviewed only, unrun (ADR-0033); only EAK ComponentDef JSON is imported (no external format translation, no authoring wizard, no bulk matching); metadata per machine |
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
