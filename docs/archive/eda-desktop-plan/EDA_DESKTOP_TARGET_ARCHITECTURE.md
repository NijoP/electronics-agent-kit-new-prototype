# EDA desktop target architecture

The architecture EAK grows into to meet the 2026-11-14 master directive, starting from what exists
on `main` (see [`EDA_DESKTOP_GAP_ANALYSIS.md`](EDA_DESKTOP_GAP_ANALYSIS.md)). It keeps every
existing guarantee: one canonical EAK-IR, every mutation through `Engine::execute`, revision checks,
journalled transactions, diff-based undo/redo, independent revision-bound verification, and a
manufacturing release gate that never says "manufacturing-ready".

This is a design target, not a description of code. Items marked **ADR needed** change a shared
contract (EAK-IR, `Command`, `CommandResponse`, report types or IPC types) and need an ADR before
they are built (AGENTS.md, "Module ownership"). Lessons taken from the EasyEDA ecosystem are cited
from [`docs/research/EAK_COMPETITIVE_ARCHITECTURE.md`](../research/EAK_COMPETITIVE_ARCHITECTURE.md).

## Principles

1. **Rust owns engineering state.** The frontend is presentation and interaction. It may hold
   *ephemeral interaction state* (hover, an in-progress drag, the rubber-band preview, which panel is
   open) but never a second copy of design data it can edit.
2. **Documents are views, not files.** A document tab is a typed view onto the one `Project`; it
   never has its own design model.
3. **One gesture, one command, one undo step.** A drag that moves five parts and rubber-bands twelve
   wires is a single compound command and a single revision.
4. **Long work is a job.** Routing, verification, export, simulation and agent runs execute off the
   UI thread as cancellable jobs that report progress and apply their result as a command against the
   revision they were computed from (stale results are refused, not merged).
5. **Disabled, not faked.** Every menu item and button maps to a registered action whose
   availability is computed from real state; unimplemented actions are visible as disabled with a
   reason string.
6. **Learn from EasyEDA, do not depend on it.** Concepts from the EasyEDA ecosystem are re-implemented
   natively; no EasyEDA code, assets or runtime are used (see `docs/research/`).

## Layering (target)

```
apps/desktop (React/TS)                       presentation + interaction only
  shell: menu bar, toolbar, dock layout, tabs, status bar, action registry, themes
  editors: SchematicEditor, PcbEditor, LibraryEditor, SimulationView, OutputsView
  panels: ProjectTree, ObjectTree, NetExplorer, ComponentExplorer, Inspector,
          LayerManager, Console, Findings, Jobs, AgentActivity, Assistant
  render: SVG today; per-editor renderer interface so PCB can move to Canvas2D/WebGL after benchmarks
        |  Tauri IPC (typed requests) + Tauri events (engine events, job progress)
apps/desktop/src-tauri                        thin adapters, no engineering logic
  session: open project, document registry, workspace persistence, job manager handle
        |
eak-runtime    Engine::execute (unchanged contract) + Command (extended) + jobs module (new)
eak-core       EAK-IR v2: layer stack, via spans, new primitives, component evidence
eak-engineering netlist/connectivity, copper, router (N layers), zone fill, placer, spatial index
eak-validation ERC / DRC / MECH / DFM (new), revision-bound
eak-manufacturing Gerber/Excellon N layers, BOM, CPL, GenCAD (new), release gate
eak-intelligence agent loop, tools, context builder (new), proposal/preview
eak-simulation (new, optional) SPICE netlist export, external simulator adapter, waveform data
```

Dependencies stay one-directional (ARCH-01). `eak-simulation` depends on `eak-core` and
`eak-engineering` (netlist) only.

## 1. Document model

**What a document is.** A `DocumentRef` names a view onto the project:

| Kind | Identity | Backed by |
|---|---|---|
| `Home` | project id | project metadata, requirements, recent activity |
| `Schematic` | sheet id (`sht_...`) | `Project.schematic` filtered by sheet |
| `Pcb` | `pcb` (one board per project today) | `Project.pcb` |
| `Library` / `Symbol` / `Footprint` | component id | `Project.library[id]` or a user/built-in library item |
| `Simulation` | simulation profile id | `Project.simulation` (new, optional section) |
| `Outputs` | output revision (`rev-N`) | `<project>/output/rev-N` manifest |

**Where tab state lives.** Open tabs, active tab, per-tab viewport, panel layout and theme are *UI
workspace state*, not design data. They are stored outside EAK-IR in a non-canonical file
`<project>/.eak-workspace.json` (rebuildable; deleting it loses only layout) written by
`src-tauri`, never by the engine. This keeps EAK-IR free of UI concerns and keeps `project.eak.json`
byte-stable when the user only pans.

**Dirty, Save and Save As in a journalled engine.** EAK commits every command durably, so there is
no unsaved design data to lose. The directive's requirements map as follows (**ADR needed**):

- *Save* (Ctrl+S) creates a named checkpoint through the existing checkpoint store and records the
  revision as the "saved revision".
- *Dirty indicator* = the current revision differs from the saved revision (per project; a tab shows
  it when its document's section changed since that revision, using the section deltas).
- *Save As* copies the project folder (canonical file, journal, checkpoints, evidence) to a new
  folder and opens the copy.
- *Close with changes* offers "Save checkpoint", "Close" and "Cancel"; nothing is ever discarded,
  because the journal already holds it. Unfinished interactions (mid-route, mid-drag) are cancelled
  explicitly first.

**Backend:** `src-tauri/src/documents.rs` (new) with `list_documents`, `open_document`,
`close_document`, `save_checkpoint`, `save_as`; documents are derived from the `Project` on demand.
Several projects open at once is not required by the directive and is deferred; one session keeps
one `Engine`.

## 2. EAK-IR v2: layers, stack-up and via spans (ADR needed)

Replaces the two-value `CopperLayer` enum. The directive (Part 6.2) is explicit that this must be an
engine change, not frontend labels.

```
LayerId        stable string id, e.g. "cu.top", "cu.in1".."cu.in30", "cu.bottom",
               "mask.top", "paste.top", "silk.top", "outline", "mech.1", "doc.1", "asm.top", ...
Layer          { id, name, kind: Copper|SolderMask|Paste|Silkscreen|Outline|Mechanical|Documentation|Assembly|Drill,
                 side: Top|Bottom|Inner|None, order, enabled, visible_default, colour, output_id }
StackupEntry   Copper { layer: LayerId, thickness, copper_oz } | Dielectric { thickness, material, er, kind: Core|Prepreg }
Stackup        { entries: Vec<StackupEntry>, board_thickness (derived and checked) }
ViaSpan        { from: LayerId, to: LayerId }    through = outermost pair
ViaKind        Through | Blind | Buried | Micro  (derived from span; validated against the fabrication profile)
Track.layer    LayerId   (copper layers only; the integrity gate rejects others)
Zone.layer     LayerId
Via            + span: ViaSpan
Pad (THT)      spans all copper layers; SMD pads sit on the outer copper layer of the footprint side
```

- `Board` gains `layers: Vec<Layer>` generated from the stack-up for copper and fixed per side for
  mask/paste/silk; mechanical/documentation layers are user-added.
- Allowed copper counts: 1 (single-sided), 2, 4, 6, 8; the type allows more, and the fabrication
  profile limits what is accepted.
- **Migration v1 -> v2** (`eak-core/src/migrate.rs`): `top` -> `cu.top`, `bottom` -> `cu.bottom`,
  every via becomes a through span `{ cu.top, cu.bottom }`, and `Stackup { copper_layers: 2, ... }`
  becomes the explicit two-copper stack-up with the same thickness and copper weight. A v1 file whose
  stack-up says `copper_layers != 2` cannot be represented faithfully and is **rejected with an
  error**, not guessed (the directive: "unknown layer configurations fail safely"). PR #4's
  `StackupLayer` work is the natural starting point and should be reconciled rather than duplicated.
- Every consumer moves from `CopperLayer` to `LayerId` with the stack-up as the authority:
  `copper.rs` (shapes per layer, connectivity through vias by span), `router.rs` (layer graph from the
  stack-up, via cost by span), `drc.rs` (per-layer clearance, via span validity, annular ring per
  layer), `gerber.rs`/`drill.rs` (one copper file per copper layer, drill files per span),
  `scene3d.rs`, agent tool schemas, IPC types and `types.ts`.

## 3. Command extensions (ADR needed)

Today's 38 commands stay. New variants, grouped by phase in the implementation plan:

- **Compound:** `Batch { commands: Vec<Command>, label }` applied atomically on one clone, with one
  integrity check, one journal entry and one undo step. Used by drag-move with rubber-banding,
  copy/paste, align/distribute and multi-delete. Its risk class is the maximum of its parts.
- **Schematic primitives:** `AddJunction`, `AddBus`/`AddBusEntry`, `AddNetPort` (or extend
  `LabelKind` with `Port`/`Global`), `AddText`, `AddGraphic { shape: Rect|Polygon|Arc|Line }`,
  `MirrorSymbol`, `MoveWireEnd`, `SetSymbolProperty { key, value }`.
- **PCB primitives:** `PlaceFootprint` (without schematic sync, for mechanical parts),
  `MoveTrackSegment`, `SetTrackWidth`, `AddBoardText`, `AddKeepout { layers, kinds }`,
  `AddRuleRegion`, `AddDimension`, `SetZoneProperties { thermal, island_removal, priority }`,
  `RefillZones` (records fill parameters; the fill itself stays derived).
- **Layers:** `SetStackup` (extended to the v2 stack-up), `AddUserLayer`, `SetLayerProperties`
  (name, colour, default visibility, enabled).
- **Library:** `UpsertComponentDefinition` stays the write path; add `SetComponentEvidence` for
  per-field evidence (section 6).

Each new variant needs an `OperationClass`, the integrity checks it relies on, an agent tool schema
(`eak-intelligence/src/tools/schema.rs` has an exhaustive `match` that forces this), undo coverage in
the property test, and a `types.ts` mirror. Generating the TypeScript types from the Rust `serde`
shapes is recommended to stop hand-mirroring drift.

## 4. Job system

New `eak-runtime/src/jobs.rs`:

```
JobId, JobKind { Route, Verify(Check), Export, Simulate, Agent, Library }
JobHandle  { id, cancel: Arc<AtomicBool>, progress: Arc<Mutex<Progress>> }
Progress   { fraction: Option<f32>, stage: String, messages: bounded ring }
JobResult  { base_revision, outcome: Commands(Vec<Command>) | Report | Files | Error }
```

- A job snapshots `Project` at `base_revision` (a `Clone` of the IR) and runs on a worker pool sized
  for modest hardware (default 2 workers).
- Computation never mutates the project. Results that change the design are submitted as one
  `Batch` through `Engine::execute` with `expected_revision = base_revision`; if the user edited in
  the meantime, the apply is refused as stale and the user can re-run.
- `src-tauri` forwards progress as Tauri events (`job://progress`, `job://done`); the Jobs panel and
  status bar read them. Cancellation sets the flag; the router and fill loops check it.
- External processes (a SPICE simulator, later) reuse the existing `process.rs` supervisor
  (timeouts, tree kill, bounded output) inside a job.

## 5. Editor interaction architecture (frontend)

- **Action registry** (`apps/desktop/src/actions/`): one table of actions with id, label, shortcut,
  menu path, toolbar slot, `isEnabled(state) -> true | { disabled: reason }` and `run`. Menus,
  toolbar, palette and keyboard shortcuts are generated from it, so nothing can be visible without an
  implementation behind it. A test asserts that every visible action is either enabled with a handler
  or disabled with a reason.
- **Tool state machine** per editor (select, place, wire, route, zone, measure...): pure TypeScript
  reducers over pointer and key events that produce either preview geometry (ephemeral) or a
  `Command`/`Batch` to submit. Pure reducers are unit-testable without a DOM.
- **Selection** is a set of stable IR ids held in the frontend; cross-probing and net highlighting
  ask the backend (`related_objects(ids)`, `net_members(net)`) so the relationship logic stays in
  Rust (`eak-engineering::netlist` and `copper::analyse`).
- **Viewport** per document tab, persisted in the workspace file.
- **Hit testing and snapping** on derived geometry the backend sends (pins, pads, track segments,
  wire end points), with a client-side uniform-grid index rebuilt on each snapshot; snapping targets
  are pins, pads, wire/track end points and the grid.
- **Renderer interface:** keep SVG for the schematic. For the PCB, define a `PcbRenderer` interface so
  a Canvas2D (or WebGL) implementation can replace SVG if the Phase 8 benchmark shows SVG cannot hold
  interactive frame rates on a large board (target: 2,000 footprints and 20,000 track segments on the
  8 GB reference host). No renderer change without that benchmark (Part 12).
- **Dock layout:** a small in-house split/tab layout (rows/columns of resizable panes holding panel
  ids) serialised to the workspace file, rather than a heavy docking dependency; any new npm
  dependency is recorded in `LICENSING.md` with its licence.

## 6. Component library and evidence

- Three sources merged by a backend `LibraryService`: built-in (`eak-core/src/builtin.rs` and data
  files), user library (a folder of EAK component files in the OS app-data directory), and project
  library (`Project.library`, the self-contained copy). Placing a part copies its definition into the
  project.
- `ComponentDef` gains per-field evidence (**ADR needed**): `evidence: BTreeMap<FieldPath, Evidence>`
  where `Evidence { source_url, document_revision, page, retrieved_at, sha256, method: Datasheet|Manual|AiCandidate|Supplier, verified_by: Option<Human> }`.
  A component is `Verified` only when manufacturer, MPN, package, pin count, pin numbers and
  functions, pad geometry and the pin-to-pad map all carry verified evidence; otherwise it is
  `Unverified` and the UI says so. AI output is always `AiCandidate` until a human approves it and the
  deterministic checks pass (pin count matches, pad count matches the pin map, pad pitch and size are
  consistent with package-drawing numbers captured as evidence).

## 7. Verification workflow

Unchanged contract: ERC/DRC/MECH (and a new DFM report) bound to revision and input hash, with
`not_implemented` and `limitations`. Additions: verification runs as a job; the Findings panel binds
each finding to its document and object ids (already present per ADR-0009) and the shell implements
navigate, select, zoom-to-bounding-box and highlight. A "safe correction" is offered only where the
finding's `suggested_fix` maps to a concrete `Command` that the user previews and approves.

## 8. AI integration

- `eak-intelligence` gains a **context builder** that serialises the active document, selection, net,
  layer, latest reports and manufacturing status into the agent's context (read-only).
- The agent runs as a job. Its proposals are `Batch` commands previewed on a sandbox clone (the
  existing preview path), shown as a diff in the Assistant panel, applied only on approval, then
  verified by `eak-validation` (never by the model). Rollback is undo of that single revision.
- The Agent Activity panel streams the existing `AgentRun` log (task, tool calls, approvals,
  results, errors); persisting it to `evidence/` closes the STATUS.md gap.

## 9. Simulation (Phase 7)

- New optional crate `eak-simulation`: SPICE netlist generation from the EAK netlist plus per-
  component simulation models (a new optional `ComponentDef.sim_model` with provenance), analysis
  definitions (OP, DC sweep, AC, transient) and a waveform data model with CSV export.
- Execution through an **external** simulator process found on the user's machine via `process.rs`,
  not bundled until its licence and distribution terms are reviewed (see
  [`LICENCE_AND_ATTRIBUTION_AUDIT.md`](../research/LICENCE_AND_ATTRIBUTION_AUDIT.md)). ngspice is the
  primary candidate. If no simulator is installed, the Simulation workspace shows that state and its
  run actions stay disabled with the reason.
- The waveform viewer is a native frontend component (multiple traces, zoom/pan, cursors).

## 10. Manufacturing

Gerber and Excellon become N-layer (one copper Gerber per copper layer, drill files per via span);
add GenCAD export, an assembly drawing and a fabrication report. The release gate stays:
"Verification passed" and "Release eligible" only when revision-bound ERC/DRC/MECH/DFM have zero
errors for the exact revision; otherwise DRAFT. Generating files never implies readiness.

## 11. Performance on modest hardware (Part 15)

- Incremental rendering: React keys per IR id so a command re-renders only changed items; IPC moves
  from "full snapshot after every command" to "snapshot on open plus section deltas" once profiling
  shows the full snapshot is a bottleneck (measure first).
- Spatial indexing in Rust for DRC and routing (a uniform grid or R-tree over copper shapes) and in
  TypeScript for hit testing.
- Bounded queues for events and job progress (the event log already caps at 200 in the UI).
- Reproducible benchmarks on generated boards of fixed sizes, with numbers recorded in
  `TEST_RESULTS.md`.

## 12. Testing levels (Part 13)

- **Level 1, Rust:** migration v1->v2 golden files; N-layer connectivity, routing and DRC; via span
  validation; `Batch` atomicity and undo; job cancellation and stale-result refusal.
- **Level 2, frontend:** tool reducers (pure), action-registry completeness, dock layout
  persistence, tab/document behaviour with mocked IPC.
- **Level 3, real desktop:** drive the built Tauri app through WebDriver (`tauri-driver`), which Tauri
  supports on Linux (WebKitWebDriver) and Windows (Edge WebDriver). Tauri's WebDriver support does not
  cover macOS (WKWebView has no WebDriver), so macOS Level 3 needs another approach and is a recorded
  limitation until solved. The 16-step scenario in Part 13 becomes one scripted test with screenshots
  stored as evidence. These tests need a display; on headless Linux they run under a virtual X server.

## Shared-interface ownership

| Interface | Owner (one writer at a time) | Consumers |
|---|---|---|
| EAK-IR types, schema version, migrations (`eak-core`) | IR owner (one worker per phase) | every crate, IPC, `types.ts` |
| `Command` enum and `apply` (`eak-runtime/src/command.rs`) | runtime owner | intelligence tools, desktop, CLI |
| Report / `Finding` (`eak-validation/src/report.rs`) | validation owner | desktop findings panel, agent |
| IPC types (`src-tauri/src/lib.rs`) and `types.ts` | desktop owner | frontend |
| Job API (`eak-runtime/src/jobs.rs`) | runtime owner | desktop, intelligence, simulation |

Workers on different areas coordinate changes to these files through an ADR and a single owner per
phase; two workers must not edit `ir.rs` or `command.rs` concurrently (directive Part 14).
