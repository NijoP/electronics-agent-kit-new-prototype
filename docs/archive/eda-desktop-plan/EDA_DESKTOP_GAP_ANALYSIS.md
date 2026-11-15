# EDA desktop gap analysis (Phase 0)

What EAK has today against the owner's master directive of 2026-11-14 ("Transform EAK into a
professional, fully interactive EDA desktop application"). This document only describes; the target
design is in [`EDA_DESKTOP_TARGET_ARCHITECTURE.md`](EDA_DESKTOP_TARGET_ARCHITECTURE.md) and the
sequence of work is in [`EDA_DESKTOP_IMPLEMENTATION_PLAN.md`](EDA_DESKTOP_IMPLEMENTATION_PLAN.md).

## Snapshot and method

- **Baseline:** `origin/main` at `afd9351` ("fix(runtime): guard kill_tree; relicense MIT OR
  Apache-2.0; professional README (#19)"), read on 2026-11-14.
- **Method:** the source files were read directly (the line counts below come from `wc -l` on that
  commit). No build or test was run for this audit; four other workers share the 8 GB host. The test
  baseline is taken from the merged PR bodies and `STATUS.md` (see [Test baseline](#test-baseline)).
- **Status words used here:** *works* (implemented, with a test listed in STATUS.md or a PR body),
  *partial* (some code exists, the directive's requirement is not met), *missing* (no code),
  *architecturally missing* (cannot be added without changing a shared contract: EAK-IR, `Command`,
  IPC types or report types).

### Update after the snapshot (2026-11-14, later the same day)

Two of the in-flight items in the table below merged into `origin/main` after this analysis was
written, moving `main` from `afd9351` to `6a3b358`. **This document has not been re-audited against
them**; the gap tables below still describe `afd9351`. The summaries here come only from the two PR
bodies and the diff file list, not from reading or running the code:

- [PR #22](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/22) (merge `21dfe5e`),
  "multilayer boards": its body reports `CopperLayer::Inner(n)`, a 2-16 copper-layer stack-up with
  signal/plane kinds, a `SetLayerCount` command, through/blind/buried via spans, inner plane zones with
  thermal reliefs, DRC-016/017, per-layer Gerber and per-span drill export, a Fabricator stack-up check
  (limits unverified), and PCB editor layer controls; it merges and extends PRs #4 and #5 (ADR-0012). Its own
  listed gaps: layer name/colour not stored per layer, no microvias, layer manager lists copper only,
  autorouter outer layers and through vias only.
- [PR #21](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/21) (merge `6a3b358`),
  "professional workspace shell", frontend only: its body reports an activity bar, collapsible panels,
  editor tabs (Schematic+PCB / Schematic / PCB), a bottom panel, a status bar, command palette and
  quick open, dark and light tokens, and a local deterministic assistant that proposes typed commands.
  Its own listed gaps: no menu bar, tabs not bound to backend document identity, layout not dockable
  or persisted, not launched in a real Tauri window.

Consequences for this audit: the Part 4 and Part 6 rows that say *missing* for tabs, themes, status
bar, multi-layer IR, via spans, thermal reliefs and the layer controls are likely out of date, and the
in-flight table below is superseded for the shell and multi-layer rows. The supplier work
([PR #17](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/17)) and
`fm/eak-install-scripts` were still unmerged at `6a3b358`. A re-audit of Parts 4 and 6 at the new
`main` is the first item of plan Phase 1.

### Work in flight at the time of reading (pending, not part of this baseline)

The owner has other workers on the same directive. Their branches were not read for this audit and
nothing here assumes they land:

| Area | Where | State on 2026-11-14 |
|---|---|---|
| Desktop workspace shell (`apps/desktop`) | another worker | in progress, no PR opened yet when this was read |
| Multi-layer engine (EAK-IR layers) | another worker | in progress, no PR opened yet when this was read |
| Supplier integrations | another worker; earlier [PR #17](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/17) (Distributor + Fabricator prep) open, conflicting | pending |
| Install scripts | branch `fm/eak-install-scripts` on origin | pending, no PR yet |

Older open PRs, all conflicting with `main` unless noted:
[#1](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/1) dev method (mergeable),
[#3](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/3) intelligence core (its content already reached `main` through #16),
[#4](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/4) per-layer stackup, EAK-IR schema v2,
[#5](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/5) thermal reliefs, silkscreen refdes, DRC-014/015,
[#6](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/6) buck power calculations,
[#7](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/7) library expansion,
[#8](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/8) offline Distributor sourcing records,
[#9](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/9) ESP32-C3 validation board,
[#10](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/10) buck validation board (mergeable),
[#11](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/11) `eak` CLI and CI fixes.
PR #4 is the closest prior work to the multi-layer upgrade (Part 6.2): it adds a per-layer
`StackupLayer` list and a v1 to v2 migration but explicitly keeps more than two copper layers as a gap.

## What exists on `main`

| Crate / path | Size | What it does today |
|---|---|---|
| `crates/eak-core` | `ir.rs` 366, `library.rs` 200, `geom.rs` 335, `integrity.rs` 117, `migrate.rs` 53, `builtin.rs` 389 | EAK-IR schema v1: project, requirements, project-local library, schematic (sheets, symbols, wires, net labels with `Net`/`Power` kind, no-connects), PCB (board outline + `Stackup`, footprints, tracks, vias, zones), constraints and net classes, manufacturing config. Integer nanometres, quarter-turn rotations. 7 built-in generic components. |
| `crates/eak-runtime` | `command.rs` 1159, `engine.rs` 674, `delta.rs` 131, `store.rs` 178, `process.rs` 298, `tasks.rs` 162, `index.rs` 106 | 38-variant `Command` enum, risk classes, `Engine::execute` (revision check, approval, apply on clone, integrity gate, section diff, fsync'd journal, atomic write, events), undo/redo as new revisions, checkpoints, lock, crash reconciliation, process supervisor (timeout, cancel, bounded output), task board, SQLite recent index. |
| `crates/eak-engineering` | `copper.rs` 530, `router.rs` 472, `netlist.rs` 232, `ipc.rs` 312, `placer.rs` 148, `calc.rs` 146, `scene3d.rs` 93 | Netlist resolution, exact copper shapes and connectivity, zone fill on a 0.1 mm raster, native two-layer grid A* router (8-way + vias, MST order, exact re-check), constructive placer, IPC-2221 calculators, circuit calculators, 3D scene data. |
| `crates/eak-validation` | `erc.rs` 508, `drc.rs` 653, `mech.rs` 104, `report.rs` 251 | ERC-001..013, DRC-001..013, MECH-001..003. Reports bound to revision + SHA-256 input hash, with `not_implemented` and `limitations`; findings carry stable ids, affected objects, location, measured vs required and suggested fix (ADR-0009). |
| `crates/eak-manufacturing` | `lib.rs` 299, `gerber.rs` 331, `drill.rs` 107 | Gerber RS-274X (copper, mask, paste, silk per side, outline), Excellon, BOM CSV, CPL CSV, SHA-256 manifest, release gate (DRAFT unless revision-bound ERC/DRC/MECH all pass), package re-verification. |
| `crates/eak-intelligence` | `agent.rs` 747, `tools/*` ~1400, `provider/*` ~1770, `domain/*` ~1200 | Provider trait (Anthropic, OpenAI-compatible, Ollama), adaptor (list models, test connection), one typed tool per `Command`, read-only tools, approval-gated agent loop (Manual/Assisted/Autonomous), DESK domain classifier (100 classes). |
| `apps/desktop/src-tauri` | `lib.rs` 657, `models.rs` 409 | IPC: create/open/close/current project, `execute_command` (Human actor + rendered revision), undo, redo, events, recovery ack, `run_check` (ERC/DRC/MECH evidence), domain classification, built-in library list, manufacturing export, recent projects, 7 `model_*` commands. The `Snapshot` sent to the UI is the whole `Project` plus derived pin/pad geometry. |
| `apps/desktop/src` | `SchematicView.tsx` 355, `PcbView.tsx` 318, `Workspace.tsx` 278, `ModelSettings.tsx` 262, `Canvas.tsx` 101, others < 115 | React 19 + Vite, no UI library. One workspace screen: toolbar, schematic and/or PCB view side by side, Engineering Monitor (findings), Domain panel, Export panel, activity log, command palette (Ctrl/Cmd+K). SVG `Canvas` with wheel zoom about the cursor and drag pan. |

Shared contracts that matter for the directive: `eak_core::Project` and every type under it,
`eak_runtime::Command` / `CommandResponse` / `Event`, `eak_validation::report::{Report, Finding}`,
and the IPC types in `apps/desktop/src-tauri/src/lib.rs` (mirrored by hand in `apps/desktop/src/types.ts`).

## Gap analysis by directive part

### Part 4: desktop shell and workspace

| Requirement | Today | Status |
|---|---|---|
| Application menu (File, Edit, View, Project, Place, Design, Tools, Route, Simulation, Manufacturing, AI Assistant, Settings, Help) | No menu bar. A toolbar in `Workspace.tsx` and the command palette in `CommandPalette.tsx`. | missing |
| Main toolbar (new/open/save/save-as/undo/redo/select/move/rotate/delete/zoom/fit/grid/rules/ERC/DRC/route/sim/export) | Toolbar has undo/redo, layout toggle, ERC/DRC/MECH, export, close. Per-editor tool strips in `SchematicView`/`PcbView`. No save-as (every command is saved durably), no grid configuration, no design-rule editor, no simulation. | partial |
| Real document tabs (home, schematic sheet, PCB, library/symbol/footprint editor, simulation, manufacturing) | One workspace with a `layout` state of `both` / `schematic` / `pcb` (`Workspace.tsx`). No document identity beyond the project; the schematic view switches sheets with a `<select>` (`SchematicView.tsx`), but there is no "add sheet" control and no per-sheet tab. | architecturally missing (no document model in IPC; see target architecture) |
| Dirty indicator, save before close, restore recent documents, per-tab viewport | Every command is durably committed, so "dirty" has a different meaning (unsaved = never; see target architecture). Recent projects exist (SQLite index). Viewport is component state and is lost on remount. | partial |
| Dockable, resizable, persisted panels; dark and light themes | Fixed CSS grid in `styles.css` (118 lines), one dark palette. | missing |
| Project manager, object tree, net explorer, component explorer, properties inspector, console, task manager | Engineering Monitor (findings), activity log of engine events, Domain panel. No tree, explorer or inspector; ref/value editing is inline in the schematic tool options. | missing |

### Part 5: schematic editor

Tools today (`SchematicView.tsx`, `type Tool = "select" | "place" | "wire" | "connect" | "label" | "move" | "delete"`):
place library part, wire with pin snap, connect two pins, net/power label, two-click move,
delete, rotate `R`, edit ref/value. Undo/redo through the engine; Delete, Escape, Ctrl+Z,
Ctrl+Shift+Z work.

| Requirement | Status | Note |
|---|---|---|
| Bus, bus entry, net port, junction object, text, rectangle, polygon, arc, measurement | missing | No IR types for bus, port, junction, text or graphics. |
| Mirror | architecturally missing | `SymbolInstance` has `rotation` only; no mirror flag. |
| Copy/paste, multi-select | missing | Commands act on one object; no batch command. |
| Drag-to-move, rubber-band wires on move | missing | Move is two clicks; wires keep their end points (STATUS.md limitation). |
| Junction detection, electrical connection detection, net highlighting | partial | `eak-engineering::netlist::build` resolves nets; the UI does not highlight a net. |
| Cross-sheet connectivity, hierarchy | partial / missing | IR has multiple sheets and `AddSheet`; labels join by name across sheets in `netlist.rs`; the UI shows one sheet at a time via a selector; no hierarchical sheets (ERC lists this as not implemented). |
| Reference designator annotation | works | `Command::Annotate`. |
| Search by reference/value, selection through object tree | missing | |
| `W` wire, `Space` rotate during placement, Ctrl+S | missing | `R` and Delete exist. Ctrl+S has nothing to do because every edit is saved. |

### Part 6: PCB editor

Tools today (`PcbView.tsx`, `type Tool = "select" | "outline" | "move" | "route" | "via" | "delete"`):
board outline (rectangle/polygon), two-click move, rotate, flip side, route tracks top/bottom with
45-degree and L legs, place via, delete, `L` toggles layer while routing.

| Requirement | Status | Note |
|---|---|---|
| Footprint placement | partial | Footprints only come from "Sync schematic to PCB" (`SyncSchematicToPcb`). |
| Lock/unlock | partial | `SetFootprintLocked` exists in the engine; no UI. |
| Text, dimensions, measurement, mechanical objects, keepouts, rule regions | architecturally missing | No IR types. |
| Multi-select, align, distribute, drag-and-drop move | missing | |
| **Multi-layer (6.2)** | architecturally missing | `CopperLayer` is a two-value enum (`Top`, `Bottom`) in `eak-core/src/ir.rs`; `Stackup.copper_layers: u8` exists but nothing honours a value other than 2. The enum is used in `ir.rs`, `command.rs`, `fixtures.rs`, `copper.rs`, `router.rs`, `drc.rs`, `gerber.rs`, `manufacturing/lib.rs`, the agent tools and the UI type `CopperLayer = "top" \| "bottom"`. `Via` has no span (implicitly through). DRC lists "Two copper layers only" under limitations. PR #4's per-layer stackup is unmerged. |
| Layer manager (6.3) | missing | No layer list in the IR beyond copper; mask/paste/silk/outline exist only as Gerber outputs. The UI has a two-option layer `<select>`. |
| Interactive routing (6.4) | partial | Point-to-point segments with 45/L legs and a layer toggle. No pad-start net inference beyond picking the net, no live clearance feedback, no walkaround or push-and-shove, no airwires (ratsnest), no width presets from net class in the UI. Auto-route exists (`AutoRoute`, two layers). |
| Copper zones (6.5) | partial | `AddZone`/`DeleteZone`, raster fill used by connectivity and Gerber; no zone tool in the UI, no thermal relief (PR #5 unmerged), no island removal option, no explicit refill command (fill is derived on read). |

### Part 7: component library

| Requirement | Status | Note |
|---|---|---|
| Built-in library | works (small) | 7 generic components in `eak-core/src/builtin.rs`; `library_components` IPC; place tool picker. PR #7 (more parts) unmerged. |
| Device/symbol/footprint/3D/simulation-model categories, search, filters, previews | missing | |
| Provenance and evidence per field (MPN, datasheet revision, pins, pad geometry) | partial | `ComponentDef` has `manufacturer`, `mpn`, `distributor`, `datasheet`, `provenance: String`, `Rating { source, verified }`. No per-field evidence structure, no verification state for pins/pads/package. |
| Symbol and footprint editors | missing | `UpsertComponentDefinition` exists as the write path. |
| Project / user / built-in library separation | partial | Project-local copy in `Project.library`; no user library. |
| AI datasheet-to-library workflow | missing | |

### Part 8: design manager and cross-probing

Cross-probing by `symbol_id` between schematic and PCB and from findings works (PR #12). Missing:
component explorer, net explorer, net-level cross-probe (pins + wires + pads + tracks), zoom-to-object
on the other view, track length and layer distribution per net.

### Part 9: engineering verification

ERC 13 rules, DRC 13, MECH 3 work and are revision-bound with `not_implemented` and `limitations`;
findings are structured for navigation (ADR-0009). Partial against the directive: clicking a finding
selects by `symbol_id` only; zoom-to-violation, highlight and "safe correction" are not wired.
Missing checks named in the directive and listed by EAK as not implemented today include silkscreen
clearance, copper-to-hole spacing, keepouts, impedance, differential pairs, DFM stack-up checks.
No DFM rule set exists beyond MECH and the release gate.

### Part 10: AI-native workflow

`eak-intelligence` has the agent loop, typed tools, approvals and providers, and the desktop has the
provider settings and Domain panel. Missing in the desktop: assistant panel, agent activity panel,
proposed-command preview/approval UI, project/selection context injection, run log persisted to
`evidence/` (STATUS.md "Known gaps").

### Part 11: simulation and manufacturing

Manufacturing outputs work (Gerber, Excellon, BOM, CPL, manifest, release gate) for two layers.
Missing: GenCAD, IPC-2581/ODB++, 3D output (STEP), assembly drawings, fabrication report;
simulation is entirely missing (no SPICE netlist, no simulator, no waveform view).

### Part 12: technical architecture

| Required backend service | Today |
|---|---|
| Document management | Missing; a session holds one open `Engine`. |
| Selection, editor tools, viewport state | Frontend component state only (`useState` in each view). |
| Layer management | Missing (two-value enum). |
| Component library service | `library_components` returns built-ins only. |
| Routing / verification / simulation / manufacturing jobs | Run synchronously inside one IPC call (`run_check`, `export_manufacturing`, `AutoRoute` via `execute_command`). `process.rs` (supervised child processes) and `tasks.rs` (task board) exist but are not used for in-process cancellable jobs with progress events. |
| Renderer | SVG via React; every snapshot re-renders the whole drawing; no culling, spatial index or incremental update. Adequate for the test boards (tens of parts); not measured on large designs. |
| IPC payload | Every mutation re-fetches the full `Snapshot` (entire `Project` JSON plus geometry). Fine at today's sizes; not measured at scale. |

### Part 13: testing

Level 1 (Rust) is solid: engine, IR, validation, manufacturing, intelligence and IPC are tested
headless. Level 2 (frontend) exists as vitest + jsdom with mocked IPC. **Level 3 (real desktop
WebView tests) does not exist**: no test opens the Tauri window; STATUS.md says so for every desktop
change. One flaky test is known: `eak-runtime/tests/process.rs::captures_output_and_exit_code`
(PR #19 body, timing race suspected, not fixed).

## Test baseline

**Source:** merged PR bodies #12-#19 (read with `gh pr view <n> --repo NijoP/electronics-agent-kit-new-prototype`)
and `STATUS.md` at `afd9351`. These numbers were **not re-run** for this audit.

| PR (merged) | Rust result recorded | Frontend result recorded | Platform |
|---|---|---|---|
| [#12](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/12) desktop shell | `cargo test --workspace --release` 43 passed | vitest 18 passed | Windows 11 local; CI 8 jobs green on commit 599bf91 |
| [#14](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/14) actionable findings | 58 passed, 0 failed (`--test-threads=2`); one earlier flaky failure in `process.rs` | not run (no desktop change) | Linux |
| [#15](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/15) editors + Gerber | all pass (eak-desktop ipc 11) | 33 passed | Linux |
| [#16](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/16) model adaptor | all passed (intelligence 25 unit + 4 loopback + 13 agent; desktop ipc 8 + models 9) | 23 passed | Linux |
| [#18](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/18) DESK domain | 113 passed, 0 failed, **eak-runtime excluded** (kill_tree safety rule) | 43 passed | Linux |
| [#19](https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/19) kill_tree guard | `-p eak-runtime` all passed; full workspace run **1 failure** (`process.rs::captures_output_and_exit_code`, timing race suspected) | not run | Linux |

PR #13 was documentation only and recorded no test run. GitHub Actions is disabled (billing) for the
PRs after #12, so Windows and macOS have not been verified since commit 599bf91. The baseline to
preserve is therefore: all Rust suites pass on Linux except the known flaky process test, and 43
vitest tests pass.

## Summary of the largest gaps

1. **No document model.** Tabs, dirty state, per-document viewport and multi-sheet navigation need a
   backend notion of "document" (sheet, board, library item, simulation, output) on top of `Project`.
2. **Two-layer IR.** `CopperLayer` must become a stable layer identity with a stack-up and via spans;
   this touches every crate and is the riskiest shared-contract change.
3. **Single-object commands.** Multi-select, drag-move with rubber-banding, copy/paste, align and
   distribute need batch/compound commands that stay one undo step.
4. **Missing IR primitives.** Bus, junction, port, text, graphics, mirror, keepouts, rule regions,
   dimensions, board text.
5. **No job system.** Routing, verification, export and (future) simulation run inside one IPC call;
   no progress events or cancellation in the desktop.
6. **Workspace shell.** No menus, docking, panels, explorers, inspector or themes.
7. **No Level 3 tests.** Nothing has been exercised in a real WebView.
