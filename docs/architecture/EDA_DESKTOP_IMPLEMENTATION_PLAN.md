# EDA desktop implementation plan

Sequenced plan for the 2026-11-14 master directive, following its Part 14 phases. Inputs:
[`EDA_DESKTOP_GAP_ANALYSIS.md`](EDA_DESKTOP_GAP_ANALYSIS.md) (what exists),
[`EDA_DESKTOP_TARGET_ARCHITECTURE.md`](EDA_DESKTOP_TARGET_ARCHITECTURE.md) (where it goes) and
[`docs/research/EASYEDA_FEATURE_MATRIX.md`](../research/EASYEDA_FEATURE_MATRIX.md) (what to learn from
EasyEDA). Progress is tracked in `FEATURE_MATRIX.md`, `IMPLEMENTATION_LOG.md` and `TEST_RESULTS.md`
at the repository root.

## Ground rules for every phase

- No uncontrolled rewrite: extend the working modules (AGENTS.md "Working loop"); keep every test
  that passes today passing (baseline in the gap analysis).
- Every new mutation is a typed `Command` through `Engine::execute`; shared-contract changes get an
  ADR first (`docs/decisions/NNNN-*.md`).
- Each feature is reported at the directive's status levels: implemented, unit tested, integration
  tested, real desktop tested, engineering validated, manufacturing validated. Nothing is "done" at
  "implemented".
- Verification per change: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release`,
  `scripts/check-no-python.sh`, and for desktop changes `npm ci && npm run typecheck && npm test`.
- Visible controls without a backend are disabled with a reason (target architecture, principle 5).

## Effort estimates: basis and honesty

Estimates are **worker-days**: one focused agent or engineer working sequentially on this host
(8 GB RAM, no dedicated GPU, Linux), where Rust builds run with `CARGO_BUILD_JOBS=2` and other
workers compete for memory. Full build-and-test cycles were not timed for this audit, so the
estimates include a margin for slow cycles rather than a measured figure. Ranges are wide on purpose;
the high end applies if the shared-contract work (IR v2, `Batch`) has to be redone after review.
They are not commitments.

## Shared-interface ownership and sequencing

The two files that every phase wants to change are `crates/eak-core/src/ir.rs` (with `migrate.rs`
and `integrity.rs`) and `crates/eak-runtime/src/command.rs`. Rules:

1. **One IR owner at a time.** Schema bumps are serialised: v2 (layers and via spans, Phase 3) lands
   before any later IR additions are merged, unless the IR owner decides otherwise. Additive IR
   changes still bump `SCHEMA_VERSION` with an identity migration, because an older build would
   otherwise silently drop fields it does not know.
2. **One `Command` owner at a time.** New variants land in small PRs, each with its tool schema
   (`eak-intelligence/src/tools/schema.rs`), undo test and `types.ts` mirror.
3. **`Batch` first.** The compound command (target architecture, section 3) lands early in Phase 2,
   because drag-move, copy/paste, align and AI proposals all depend on it.
4. **Work in flight** when this was written: a desktop workspace-shell worker (this plan's Phase 1),
   a multi-layer engine worker (Phase 3.1) and a supplier-integration worker (Phase 5). This plan is
   to be reconciled with their PRs when they open; it does not assume their designs.
5. **Update, later on 2026-11-14:** the shell work merged as
   [PR #21](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/21) and the multi-layer
   work as [PR #22](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/22) (`main` at
   `6a3b358`). PR #22 already delivers much of Phase 3.1 in its own design (`CopperLayer::Inner(n)`,
   2-16 layers, via spans, `SetLayerCount`, ADR-0012) rather than the `LayerId` design proposed here,
   and PR #21 delivers part of Phase 1 (tabs, panels, status bar, themes, palette). Phase 1 therefore
   starts with a re-audit of Parts 4 and 6 at the new `main`; Phase 3.1 shrinks to the gaps PR #22
   lists (per-layer name and colour in EAK-IR, non-copper layers in the layer manager, microvias,
   inner-layer autorouting), and the `LayerId` proposal is to be weighed against ADR-0012 in an ADR,
   not assumed. The effort figures below were written before these merges and are now upper bounds
   for Phases 1 and 3.1.

## Phase 0: repository audit and research (this PR)

- Deliverables: `docs/research/*` (EasyEDA ecosystem audit, feature matrix, API/format analysis,
  competitive architecture, licence audit); `docs/architecture/*` (gap analysis, target architecture,
  this plan); root `FEATURE_MATRIX.md`, `IMPLEMENTATION_LOG.md`, `TEST_RESULTS.md` (baseline).
- No code. Effort: 1 worker-day.

## Phase 1: desktop shell and workspace

**Goal:** a professional main window whose every control is real.

| Work item | Files / modules | Backend / contract change |
|---|---|---|
| Action registry (menus, toolbar, palette and shortcuts from one table; disabled-with-reason) | new `apps/desktop/src/actions/{registry,actions}.ts`; `CommandPalette.tsx` reads the registry | none |
| Menu bar (File, Edit, View, Project, Place, Design, Tools, Route, Simulation, Manufacturing, AI Assistant, Settings, Help) | new `apps/desktop/src/shell/MenuBar.tsx` | none (unimplemented items disabled) |
| Main toolbar and status bar (revision, saved/dirty, cursor coordinates, active layer, jobs) | `shell/Toolbar.tsx`, `shell/StatusBar.tsx`, replacing the toolbar in `Workspace.tsx` | none |
| Document tabs (home, each sheet, PCB, outputs) with close, close all, dirty marker, restore | `shell/Tabs.tsx`; new `src-tauri/src/documents.rs` (`list_documents`, `open_document`, `close_document`) | IPC additions (desktop owner) |
| Save = named checkpoint; dirty = revision since saved; Save As = copy the project folder; close prompts | `src-tauri/src/documents.rs`, using the existing checkpoint store in `eak-runtime` | **ADR** (save/dirty semantics in a journalled engine) |
| Dockable, resizable, collapsible panels; layout persisted; dark and light themes | `shell/Dock.tsx` (in-house split/tab layout); `styles.css` split into theme variables; `<project>/.eak-workspace.json` written by `src-tauri` | IPC `workspace_load` / `workspace_save` |
| Panels hosted in the dock: project tree (sheets, board, library, outputs), findings (existing `EngineeringMonitor`), console/activity log, Domain, Export | move existing components into panels | none |
| Add sheet / rename sheet from the project tree | existing `AddSheet`; new `RenameSheet` | new `Command::RenameSheet` (runtime owner) |
| Native folder picker for open/create | `@tauri-apps/plugin-dialog` (new dependency: record its licence in `LICENSING.md`) | none |

Tests: registry completeness test (every visible action enabled with a handler or disabled with a
reason); tab/document vitest with mocked IPC; Rust IPC tests for documents and workspace files. A
first Level 3 smoke test (open the app, create a project, see tabs) once `tauri-driver` is set up
(Phase 8 tooling pulled forward for a smoke test only).

**Effort: 4-7 worker-days.** Risk: dock-layout scope creep; keep it in-house and minimal.

## Phase 2: schematic editor

| Work item | Files / modules | Contract change |
|---|---|---|
| `Batch` compound command (atomic, one revision, one undo step) | `eak-runtime/src/command.rs`, `engine.rs`, `delta.rs`; tool schema in `eak-intelligence/src/tools/schema.rs` | **ADR**, new `Command::Batch` |
| Multi-select (click, shift-click, marquee), drag-to-move with live preview, rubber-band wires on move | pure tool reducers in new `apps/desktop/src/tools/schematic/*.ts`; `Canvas.tsx` gains drag capture; backend helper `wires_attached(symbol_ids)` in `eak-engineering::netlist` | new `MoveWireEnd`; a drag submits one `Batch` |
| Mirror | `ir.rs` `SymbolInstance.mirror`; `Project::pin_position` honours it | IR additive (schema bump), `MirrorSymbol` |
| Junctions, bus, bus entry, net port / global label, text, rectangle/polygon/arc/line graphics | `ir.rs` new maps in `Schematic`; `netlist.rs` (junction and bus semantics); `erc.rs` (bus consistency) | IR additive (schema bump); `AddJunction`, `AddBus`, `AddBusEntry`, `AddText`, `AddGraphic`, `LabelKind::Port` |
| Copy/paste (within and across sheets) with refdes re-annotation | frontend clipboard of ids; backend `duplicate(ids, offset)` producing a `Batch` | none beyond `Batch` |
| Shortcuts: W wire, R rotate, X/Y mirror, Space rotate while placing, Esc, Del, Ctrl+Z / Ctrl+Shift+Z, Ctrl+S (checkpoint), Ctrl+C/V | action registry | none |
| Net highlighting and cross-sheet navigation (click a label to list the other sheets on that net) | backend `net_members(net)` IPC over `netlist::build` | IPC addition |
| Object tree, component explorer, net explorer, properties inspector (ref, value, footprint, MPN, custom properties) | new panels; `SetSymbolProperty` for arbitrary properties | `SymbolInstance.properties` (schema bump), `SetSymbolProperty` |
| Search by reference, value, component | backend `find(query)` | IPC addition |

Tests: `Batch` atomicity and undo property test; mirrored pin positions; junction/bus netlist tests;
tool-reducer unit tests; explorer/inspector vitest.

**Effort: 7-10 worker-days.** Depends on `Batch` (first) and an IR-owner slot for the additive bump.

## Phase 3: PCB editor and multi-layer EAK-IR

### 3.1 Multi-layer IR (highest-risk shared change; one owner)

| Work item | Files / modules |
|---|---|
| ADR: layer identity, stack-up, via spans, migration and failure policy | `docs/decisions/NNNN-multilayer-ir.md` |
| `LayerId`, `Layer`, `StackupEntry`, `Stackup`, `ViaSpan`; `Track.layer`, `Zone.layer`, `Via.span` | `crates/eak-core/src/ir.rs`; reconcile with PR #4 (`StackupLayer`) rather than duplicating it |
| Migration v1 -> v2, rejection of unrepresentable v1 files, golden-file tests | `crates/eak-core/src/{migrate,integrity}.rs`, tests |
| Copper geometry and connectivity per layer; vias connect only their span | `crates/eak-engineering/src/copper.rs` |
| Router layer graph from the stack-up, via cost by span | `crates/eak-engineering/src/router.rs` |
| DRC per layer; via span validity; annular ring per layer; inner-layer current capacity (the IPC-2221 internal constant already exists in `ipc.rs`) | `crates/eak-validation/src/drc.rs` (drop the "Two copper layers only" limitation only when tested) |
| Gerber per copper layer, drill per span, layer-to-file map in the manifest | `crates/eak-manufacturing/src/{gerber,drill,lib}.rs` |
| Commands: extended `SetStackup`, `AddUserLayer`, `SetLayerProperties` | `crates/eak-runtime/src/command.rs`; tool schemas; `types.ts` |
| IPC and UI types | `src-tauri/src/lib.rs`, `apps/desktop/src/types.ts` |

Tests: 1/2/4/6-layer fixtures; a 4-layer board routed with a blind via and verified by DRC; migration
of every existing fixture and of the end-to-end reference design with identical meaning; Gerber layer
count and drill-span tests.

### 3.2 PCB editing

| Work item | Files / modules | Contract change |
|---|---|---|
| Layer manager panel (show/hide, colour, active layer, stack-up view with dielectric and copper thickness) | new `panels/LayerManager.tsx`; reads `Board.layers` | uses 3.1 |
| Multi-select, drag-move, align, distribute, lock/unlock UI | `tools/pcb/*.ts`; existing `SetFootprintLocked` | `Batch` |
| Track editing (select a segment, drag, change width, delete); via editing (span, size) | `PcbView.tsx` split into `pcb/` modules | `MoveTrackSegment`, `SetTrackWidth`, `SetViaProperties` |
| Zones UI (draw polygon, layer, net, clearance, priority, thermal, island removal) | `tools/pcb/zone.ts`; fill options in `copper.rs` (thermal reliefs: reconcile with PR #5) | `SetZoneProperties`, `RefillZones` |
| Board text, dimensions, keepouts, rule regions | `ir.rs` additive; `drc.rs` keepout rule | IR additive (schema bump); `AddBoardText`, `AddDimension`, `AddKeepout`, `AddRuleRegion` |
| Place a footprint without a schematic symbol (mechanical parts, mounting holes) | `command.rs` | `PlaceFootprint` |

**Effort: 3.1 is 6-10 worker-days; 3.2 is 6-9 worker-days.** 3.1 must not run concurrently with any
other IR change.

## Phase 4: routing and verification

| Work item | Files / modules | Notes |
|---|---|---|
| Ratsnest / airwires from connectivity, updated per revision | `eak-engineering::copper::analyse` exposes unrouted pairs; IPC `airwires` | connected and unconnected shown distinctly |
| Interactive router: start from a pad (net inferred), 45/90-degree modes, layer switch with via insertion, width from net class, live clearance feedback, walkaround | new `eak-engineering/src/interactive.rs` (a "route step" API that computes a legal path from the last point to the cursor against exact geometry and rules); the frontend tool calls it at pointer rate with debounce | result submitted as one `Batch`; push-and-shove after walkaround is solid |
| Rip-up and re-route of a selection | reuse `router.rs` as a job | |
| Jobs module (cancellable, progress events) and Jobs panel | `eak-runtime/src/jobs.rs`, `src-tauri` events, `panels/Jobs.tsx` | `AutoRoute`, `run_check` and export move onto jobs |
| Zone refill on relevant geometry change | fill stays derived; `RefillZones` records parameters; DRC reports a stale fill if any | |
| ERC additions (pin-conflict matrix, power-net checks); DRC additions (copper-to-hole, hole-to-hole, silkscreen clearance, keepouts); DFM report against a fabrication profile | `erc.rs`, `drc.rs`, new `eak-validation/src/dfm.rs` | a check leaves `not_implemented` only with a test |
| Findings navigation: select, open the right tab, zoom to the bounding box, highlight, show measured vs required, preview a safe correction | `panels/Findings.tsx`; uses the ADR-0009 fields already on `Finding` | |
| Net-level cross-probing (schematic pins and wires plus PCB pads and tracks) | backend `net_members`; both editors highlight | |

**Effort: 10-15 worker-days** (interactive routing is the largest single item, 5-8 of those days).

## Phase 5: component intelligence

| Work item | Files / modules | Contract change |
|---|---|---|
| `LibraryService` merging built-in, user and project libraries; search and filters | new `src-tauri/src/library.rs` (or `eak-core::library` helpers); user library folder in app data | IPC additions |
| Library browser with symbol/footprint preview, datasheet link, MPN, supplier numbers | new `panels/Library.tsx` | none |
| Per-field evidence and verified/unverified state | `crates/eak-core/src/library.rs` | **ADR**; `ComponentDef.evidence` (schema bump); `SetComponentEvidence` |
| Symbol editor and footprint editor documents | new `editors/SymbolEditor.tsx`, `editors/FootprintEditor.tsx`; write through `UpsertComponentDefinition` | none |
| AI datasheet-to-library candidate flow: candidate marked `AiCandidate`, deterministic checks (pin count, pad count vs pin map, pitch/size consistency), human approval | new `eak-intelligence` tool; checks in a new `eak-validation/src/library.rs` | |
| Supplier data (LCSC) | reconcile with the supplier-integration worker and PR #17 | |

**Effort: 8-12 worker-days.**

## Phase 6: AI integration

| Work item | Files / modules |
|---|---|
| Context builder (active document, selection, net, layer, latest reports, manufacturing status) | new `crates/eak-intelligence/src/context.rs` |
| Assistant panel wired to the configured provider (from `ModelSettings`) | new `panels/Assistant.tsx`; IPC `agent_start`, `agent_cancel`, `agent_approve` in `src-tauri` |
| The agent runs as a job; the Agent Activity panel streams the `AgentRun` log | `jobs.rs`; `panels/AgentActivity.tsx` |
| Proposals as `Batch` with sandbox preview and diff, approval, independent verification, rollback by undo | existing preview path in `agent.rs`; `Batch` from Phase 2 |
| Run log persisted to `evidence/` | `agent.rs`, `eak-runtime/src/store.rs` |

**Effort: 5-8 worker-days.** End-to-end testing needs a live provider key (none was available in
earlier work, per STATUS.md).

## Phase 7: manufacturing and simulation

| Work item | Files / modules |
|---|---|
| N-layer Gerber/drill (from 3.1) plus a re-read check of generated files | `eak-manufacturing`: a small Rust Gerber/Excellon reader for self-verification (concept from `eext-gerber-viewer`; no code reuse) |
| GenCAD export; assembly drawing; fabrication report | new `eak-manufacturing/src/{gencad,report}.rs` |
| Manufacturing workspace tab (outputs per revision; release gate status "Verification passed" / "Release eligible" / DRAFT) | `editors/Outputs.tsx` |
| SPICE netlist generation and model association | new crate `crates/eak-simulation` (`netlist.rs`, `models.rs`); `ComponentDef.sim_model` (schema bump) |
| External ngspice runner through `process.rs`, disabled with a reason when not installed | `eak-simulation/src/runner.rs` |
| Waveform viewer (traces, zoom/pan, cursors, CSV export) | `editors/Simulation.tsx` |

**Effort: 9-13 worker-days.** A licence review of any simulator EAK would bundle is a prerequisite for
distribution (see `docs/research/LICENCE_AND_ATTRIBUTION_AUDIT.md`).

## Phase 8: product validation

| Work item | Notes |
|---|---|
| Level 3 desktop tests with `tauri-driver` on Linux (virtual display) and Windows | the 16-step scenario from directive Part 13, with screenshots stored as evidence; macOS recorded as a limitation unless another driver is found |
| Performance benchmarks on generated boards (render, DRC, routing, IPC snapshot size) | numbers in `TEST_RESULTS.md`; the PCB renderer decision is taken only from these |
| Recovery tests (kill during write, reopen, UNKNOWN_OUTCOME acknowledgement) through the real app | |
| Large-project tests (thousands of parts) | |
| Cross-platform validation | needs GitHub Actions (billing currently disabled) or access to Windows and macOS machines |
| Final product report, known limitations, prioritised roadmap | |

**Effort: 6-10 worker-days**, plus waiting time for cross-platform runners.

## Totals and parallelism

The sequential total is roughly 63-95 worker-days. With the shared-interface rules above, work can
run in parallel once Phase 1 starts:

- Phase 1 (shell) alongside Phase 3.1 (IR v2): different files, except `types.ts`, where the desktop
  owner merges the IR owner's type changes.
- Phase 2 needs `Batch`, and an IR slot either after 3.1 or before it with its own schema bump; the
  IR owner decides the order.
- Phase 5 (library) and the simulation half of Phase 7 (`eak-simulation`) are mostly new modules and
  can run in parallel with Phase 4.
- Phase 6 depends on `Batch` (Phase 2) and the job system (Phase 4).
- Phase 8 tooling (a `tauri-driver` smoke test) should start early so that every later phase can add
  a Level 3 test.

On the 8 GB host, more than two concurrent Rust build-and-test workers is likely to cause memory
pressure; parallel workers should stagger full test runs.

## Slice log: Phase 3.2 / Phase 4 UI items (2026-11-14, branch `fm/eak-o5-layers-findings-zones`)

Delivered against the table in 3.2 and the Phase 4 UI items. Every control calls a real backend command or is display-only state (stated per row):

| Item | What shipped | Backend contract |
|---|---|---|
| Layer manager panel | Side view "PCB Layers": active layer, show/hide, colour per layer (display state, persisted per project in localStorage), layer count, per-layer signal/plane type, stackup table (copper oz, dielectric gap, er, board thickness) | `SetLayerCount`, `SetStackup` (existing, from PR #22); read from `Board.stackup` |
| Findings navigation | Click an ERC/DRC finding: opens the right document tab (DRC to PCB, ERC to the schematic), selects the owning part, **PCB only**: zooms to the bounding box of the named objects and location, highlights them, pulses a marker, and expands measured vs required and the suggested fix | none (reads `Finding.measured/required/suggested_fix/location`, already produced by eak-validation) |
| Airwires / ratsnest | PCB draws the engine's unrouted connections as dashed lines; toggle and count in the layer manager; only drawn when the report's revision equals the project's | new read-only IPC `connectivity` (glue over `eak_engineering::copper::analyse`, the analysis DRC-004 uses); `tests/ipc.rs` covers airwires present before routing, gone after `AutoRoute` |
| Zone drawing UI | PCB "Zone" tool (key 7): layer, net, optional clearance, corners then Enter/Finish; delete tool removes a zone | `AddZone`, `DeleteZone` (existing) |

Not done in this slice (still Phase 3.2/4): ERC finding zoom/highlight in the schematic canvas (owned by the schematic worker), zone thermal/priority/island/refill controls (need `SetZoneProperties`, `RefillZones`), non-copper layers (mask/silk/paste are not in the engine stackup), copper-weight and dielectric editing (only layer count and type are edited), multi-select/align, track and via editing, safe-fix buttons on findings, a real-window (Tauri) test.
