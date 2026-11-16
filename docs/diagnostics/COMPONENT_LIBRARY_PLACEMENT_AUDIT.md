# EAK Component Library & Placement Audit

## 1. Executive Summary

This diagnostic audit documents the root causes of the component drag-and-drop placement failure in EAK, inspects the current library and schematic architecture, and establishes the concrete fix plan for drag-and-drop, click-to-place, external library import (KiCad symbols and footprints), and agent-assisted component acquisition.

---

## 2. Root Cause Analysis: Drag-and-Drop Placement Failure

### 2.1 The Symptom
In the EAK Project / EDA workspace:
- The Library / Parts sidebar lists registered libraries, categories, and component entries (e.g. `Capacitor 0603 (MLCC)`).
- When a user attempts to click and drag a component card from the Library / Parts panel onto the schematic canvas, nothing happens. No drag ghost appears, dropping on the sheet does not place a symbol, and no transaction is created.

### 2.2 Root Causes Identified

1. **Missing HTML5 Drag Attributes in Library Panels (`apps/desktop/src/shell/LibraryPanel.tsx`, `apps/desktop/src/PartsPanel.tsx`)**:
   - The component list in `LibraryPanel.tsx` renders parts as `<button>` elements inside `<ul className="tree">`.
   - None of the component list elements declare `draggable={true}` or define an `onDragStart` handler.
   - Consequently, when the user initiates a mouse drag on a component item, the browser defaults to native text/button selection rather than initiating an HTML5 drag-and-drop data transfer.

2. **Missing Drag-and-Drop Event Handlers on SVG Canvas (`apps/desktop/src/Canvas.tsx`)**:
   - The primary canvas `<svg>` container in `Canvas.tsx` registers standard pointer event handlers (`onPointerDown`, `onPointerMove`, `onPointerUp`, `onPointerCancel`, `onWheel`).
   - However, it registers **no HTML5 Drag-and-Drop event handlers**: `onDragEnter`, `onDragOver`, `onDragLeave`, or `onDrop`.
   - Under the W3C Drag and Drop specification, an element **cannot accept drops** unless `event.preventDefault()` is invoked on the `dragover` event and `event.dataTransfer.dropEffect` is set. Without this, browsers refuse to fire the `drop` event altogether.

3. **Missing Drop & Drag Preview Pipeline in Schematic View (`apps/desktop/src/SchematicView.tsx`)**:
   - `SchematicView.tsx` possesses internal state for moving already-placed symbols (`drag?.mode === "move"`), but lacks any handler for drops originating from the external Library sidebar.
   - There is no drag state machine tracking external drags entering the canvas viewport.
   - The client mouse coordinates (`clientX`, `clientY`) during a drag are not transformed through the canvas's zoom/pan viewport matrix (`fromView`) into canonical schematic coordinates (`nanometres`), nor are they snapped to the active schematic grid (`GRID`).

4. **Incomplete Placement Tool & Preview Capabilities (`PlaceGhost`)**:
   - The existing click-to-place flow (`tool === "place"`) relies on `PlaceGhost`, which only renders a generic bounding box and crosshair. It does not render actual symbol pins, pin numbers, or orientation.
   - The placement tool hardcodes rotation to `"R0"` (`PlaceSymbol { rotation: "R0", ... }`).
   - Pressing the `'R'` keyboard shortcut during placement is intercepted by the selection rotate action, which only acts on already-committed selected symbols (`sel`), rather than rotating the symbol currently being placed.
   - The Escape key does not cancel placement mode back to the `select` tool.

---

## 3. Architectural Inspection

### 3.1 Component Library Architecture
- **Canonical IR (`crates/eak-core/src/library.rs`)**:
  - `ComponentDef` is the single source of truth for component definitions, containing:
    - `id`: Unique identifier (e.g., `"C_0603"`, `"R_0805"`).
    - `name`, `category`, `refdes_prefix`, `default_value`.
    - `symbol: SymbolDef`: Body dimensions, graphics, and pins (number, name, electrical type, relative position).
    - `footprint: FootprintDef`: Courtyard, pads (number, kind, shape, position, size, drill).
    - `pin_map: BTreeMap<String, Vec<String>>`: Exact mapping from symbol pin numbers to footprint pad numbers.
    - `model3d: Model3d`: 3D representation and package transform.
    - `provenance`: Provenance record detailing source, licence, and verification status.
  - `ComponentDef::validate(&self)` ensures structural integrity: non-empty pins, unique pin numbers, pin-to-pad coverage, and valid pad definitions.

- **Library Manifests & Catalog (`crates/eak-core/src/libmeta.rs`)**:
  - `LibraryManifest`: Contains `id`, `name`, `namespace`, `version`, `source`, `license`, `content_hash`, and `scope` (`System`, `User`, `Project`, `Imported`).
  - `LibraryMaturity`: Ordered verification levels (`Draft`, `Imported`, `StructurallyValidated`, `PinVerified`, `FootprintVerified`, `DatasheetVerified`, `EngineerReviewed`).

- **Library Storage & Search (`apps/desktop/src-tauri/src/libstore.rs`, `libcatalog.rs`)**:
  - `LibraryStore` manages local user libraries (`personal/`) and third-party libraries (`imported/`).
  - `all_defs(...)` aggregates built-in, project, personal, and imported definitions, filtering out disabled libraries.
  - Exposes Tauri commands: `library_search`, `library_footprint_candidates`, `library_catalog`, `library_set_enabled`.

### 3.2 Schematic Editor Event & Coordinate Pipeline
- **Canvas Viewport (`apps/desktop/src/viewport.ts`, `Canvas.tsx`)**:
  - The canvas displays geometry using a dynamic SVG `viewBox` representing `[x, y, w, h]` in viewport coordinates.
  - `fromView(x, y)`: Converts viewport coordinates (mm, Y down) into canonical EAK-IR coordinates (nanometres, Y up: `x * 1_000_000`, `-y * 1_000_000`).
  - `toView(Point)`: Inverts canonical nanometre coordinates into viewport millimetre coordinates (`pt.x / 1_000_000`, `-pt.y / 1_000_000`).
  - Screen client coordinates (`clientX`, `clientY`) must be transformed using the canvas SVG's bounding client rect and current `viewBox` scale.

- **Grid Snapping (`apps/desktop/src/SchematicView.tsx`)**:
  - Standard schematic grid: `GRID = 1_270_000` nm (50 mil) or 2.54 mm (100 mil).
  - `snapPoint(point, GRID, pinPositions, captureRadius)` computes the snapped point.

### 3.3 Runtime & IPC Command Execution
- **`PlaceSymbol` Command (`crates/eak-runtime/src/command.rs`)**:
  ```rust
  PlaceSymbol {
      sheet_id: Option<Id>,
      component_id: String,
      position: Point,
      rotation: Rotation, // R0, R90, R180, R270
      refdes: Option<String>,
      value: Option<String>,
  }
  ```
  - Execution sequence:
    1. Resolves `sheet_id` (or defaults to the first sheet).
    2. Invokes `ensure_component(p, &component_id)`, automatically importing from the built-in catalogue into the project library if not yet present.
    3. Calculates the next unique reference designator (`next_refdes(p, &def.refdes_prefix)`) if not explicitly specified.
    4. Resolves default value from `def.default_value`.
    5. Inserts `SymbolInstance` into `p.schematic.symbols`.
    6. Appends command and state change to transactional journal, increments project revision, updates undo stack, and invalidates geometry cache.
    7. Emits updated project snapshot to the desktop frontend.

### 3.4 Schematic-to-PCB Synchronization (`SyncSchematicToPcb`)
- `SyncSchematicToPcb` command in `crates/eak-runtime/src/command.rs`:
  - Builds netlist `nl` from schematic connections.
  - For each symbol, verifies `eak_core::pinmap::check_component(&def)`. If blocking errors exist, placement is refused with a warning.
  - Automatically stages the corresponding `FootprintInstance` into `p.pcb.footprints`, carrying `pad_nets` and reference designator.

### 3.5 External Library Import Status
- `crates/eak-core/src/kicad_import.rs`:
  - Contains parsers for KiCad 6+ S-expression formats: `import_symbol_lib` (`.kicad_sym`) and `import_footprint` (`.kicad_mod`).
  - Computes `ImportReport` with accepted items, rejected items, and uninterpreted syntax counts.
- `apps/desktop/src-tauri/src/kicad.rs`:
  - Only generates a preview report view (`KicadImportView`); currently states: `"This is a report only; nothing was added to a library."`
  - Gap: Missing the commit step that creates `ComponentDef`s and writes them to the `LibraryStore`.

---

## 4. Implementation Strategy & Source Modifications

### 4.1 Phase 1 & 2: Drag-and-Drop and Click-to-Place
1. **`apps/desktop/src/shell/LibraryPanel.tsx` & `PartsPanel.tsx`**:
   - Mark component items as `draggable={true}`.
   - Attach `onDragStart={(e) => { e.dataTransfer.setData("application/eak-component", JSON.stringify({ id: p.id, name: p.def.name })); e.dataTransfer.effectAllowed = "copy"; }}`.
   - Attach `onDragEnd`.
2. **`apps/desktop/src/Canvas.tsx`**:
   - Expose HTML5 drag event props: `onDragOver`, `onDragEnter`, `onDragLeave`, `onDrop`.
   - Prevent default on drag over to allow drops: `e.preventDefault()`.
3. **`apps/desktop/src/SchematicView.tsx`**:
   - Implement explicit drag state machine for external drops.
   - Calculate client-to-schematic coordinates on `dragOver` and `drop`.
   - Snap coordinates to grid `GRID`.
   - Render `PlaceGhost` preview following pointer during drag.
   - Support rotation during placement via `'R'` / `'r'` key (cycling `R0` -> `R90` -> `R180` -> `R270`).
   - Support Escape to cancel placement.
   - Execute `PlaceSymbol` command on drop with resolved component ID, position, and rotation.
   - Enhance `PlaceGhost` to render symbol pins, pin labels, and body outline.

### 4.2 Phase 3 & 4: External Library Import Manager
1. **`apps/desktop/src-tauri/src/kicad.rs` & `libstore.rs`**:
   - Implement `kicad_import_commit` to convert `SymbolLibImport` and `FootprintImport` into EAK `ComponentDef`s.
   - Use `eak_core::pinmap::suggest_by_number` to automatically generate pin-to-pad mappings.
   - Save the converted definitions into `LibraryStore` under `imported/` or `personal/` with a valid `manifest.json`.
   - Expose Tauri command `kicad_import_commit`.
2. **`apps/desktop/src/KicadImport.tsx`**:
   - Add "Import into Library" action allowing users to commit valid imported parts directly into their library.

### 4.3 Phase 5 & 6: Agent-Assisted Component Acquisition & PCB Sync
1. **`crates/eak-harness/src/intel.rs` & `tools.rs`**:
   - Provide typed agent tools (`propose_component`, `validate_component_definition`) that accept structured datasheet-derived component models, validate pin/pad integrity, and register them via `UpsertComponentDefinition`.
2. **PCB Synchronization Verification**:
   - Ensure placed instances maintain stable component identities and that `SyncSchematicToPcb` resolves footprint instances and net assignments seamlessly.

---

## 5. Verification & Regression Plan
- Unit tests for drag-and-drop event handling and coordinate conversion (`apps/desktop/src/tools/schematic/dragPlacement.test.tsx`).
- Click-to-place and rotation tests.
- Backend tests for KiCad symbol and footprint conversion to `ComponentDef` and storage commit.
- Verification via remote GitHub Actions CI pipeline across Linux, Windows, and macOS.
