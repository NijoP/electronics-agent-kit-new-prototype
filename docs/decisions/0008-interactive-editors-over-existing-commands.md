# ADR-0008: Interactive editors reuse the existing typed commands; two read/export IPC calls added
- **Date:** 2026-11-14
- **Context:** Outcome 1 needs schematic and 2-layer PCB editors in the desktop app. `eak-runtime`
  already has typed commands for every edit (place/move/rotate/delete symbol, wires, labels,
  outline, footprint move, tracks, vias, auto-place/route), undo/redo and per-command durable saves.
- **Options considered:**
  1. Add editor-specific commands (e.g. "DrawWirePath", "PlaceFootprint").
  2. Compose the existing commands in the UI; add only what the UI cannot get today.
- **Decision:** Option 2. **No new `Command` or IR type.** Two IPC functions were added to
  `apps/desktop/src-tauri`: `library_components` (read-only catalogue of built-in parts) and
  `export_manufacturing` (runs ERC, DRC and MECH on the current revision, stores them as evidence,
  then calls `eak_manufacturing::compile` into `<project>/output/rev-N`). Neither mutates the IR.
- **Consequences:** Footprints still come only from `SyncSchematicToPcb` (ADR-0002); the PCB editor
  moves/rotates/flips them but cannot place one without a schematic symbol. A diagonal wire or track is
  sent as several commands, one per segment, so each is undoable on its own. Export status stays
  DRAFT unless every gate passes and is never labelled manufacturing-ready.
- **Source:** Owner directive 2026-11-13 (outcome 1); Execution Instructions on the single mutation path.
