# ADR-0016: Symbol mirroring (`FlipSymbol`) and independent editor selection
- **Date:** 2026-11-15
- **Context:** Captain directive section 2 asks for schematic flip, and section 3.3 for PCB
  footprint rotate/flip/align/distribute/lock. EAK-IR could rotate a symbol but not mirror it;
  footprints already had `MoveFootprint { rotation, side }` and `SetFootprintLocked`. The same
  directive reported that dragging one of two resistors moved both: a left-drag under any
  schematic tool other than Select (and under every PCB tool) panned the whole view.
- **Options considered:**
  1. Mirror only in the desktop renderer. Rejected: pins, netlist and ERC must agree with what
     is drawn; a view-only mirror would break connectivity.
  2. Extend `Rotation` with mirrored variants (R0M ... R270M). Rejected: changes the serialized
     form of every symbol and footprint rotation.
  3. A `mirror: bool` on `SymbolInstance` (local X negated before rotation) plus one
     `FlipSymbol { symbol_id, axis }` command. **Chosen.**
- **Decision:**
  - `SymbolInstance.mirror` is `#[serde(default, skip_serializing_if = "not")]`: files written
    before this ADR load unchanged, and unmirrored symbols serialize byte-for-byte as before.
  - `SymbolInstance::place(local)` is the single local-to-sheet transform; `pin_position`, the
    netlist builder and the runtime's wire-collision checks all use it.
  - `FlipSymbol { symbol_id, axis: "horizontal" | "vertical" }` mirrors about the sheet axis
    through the symbol origin: horizontal -> rotation negated and mirror toggled; vertical ->
    rotation negated + 180 and mirror toggled. Class `ReversibleLocal`; AI tool generated like
    every other command. Attached wires are not dragged by the engine; the desktop sends a
    `Batch` with `MoveWire` steps first (as for move/rotate, ADR-0013).
  - No new PCB command: align, distribute, rotate and flip are `MoveFootprint` lists in one
    `Batch`; lock/unlock is the existing `SetFootprintLocked` (now in desktop `types.ts`).
  - Editor interaction contract (desktop only): pressing on an object and dragging moves the
    selection under Select/Place/Move (schematic) and Select/Move (PCB); pressing an unselected
    object first makes it the whole selection, so only it moves. Click = select exactly one,
    Shift = add, Ctrl/Cmd = toggle. Middle-button drag always pans. Grids are configured per
    editor kind (schematic 1.27 mm / major x4, PCB 0.5 mm / major x10) and stored as a view
    preference in localStorage, not in the project.
- **Consequences:** Library symbol graphics are still drawn as a box with pin stubs, so a mirror
  is visible through the pin positions only. Body extents are symmetric, so `half_w/half_h` do
  not change on a flip.
- **Source:** Captain IDE upgrade directive sections 2, 3.2, 3.3 (2026-11-14).
