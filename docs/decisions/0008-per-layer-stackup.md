# ADR-0008: Per-layer board stackup (EAK-IR schema v2)
- **Date:** 2026-11-13
- **Context:** IPC trace-width and current sizing need the real copper weight of the layer a
  conductor is on (the 12-24 V → 5 V 3 A buck validation board needs real copper weights for
  IPC-2152 widths and thermal). Schema v1's `Stackup` held only summary fields
  (`copper_layers`, one `outer_copper_oz` for both sides).
- **Options considered:**
  1. Add a second per-layer list next to the v1 fields (two sources of truth).
  2. Evolve `Stackup` in place to one `StackupLayer { layer, copper_oz, dielectric_after_um, er }`
     per copper layer, ordered top to bottom, and derive the layer count from it.
  3. Full multi-layer support (`CopperLayer::Inner`, via layer spans) now.
- **Decision:** Option 2. `SCHEMA_VERSION` = 2; `migrate.rs` converts v1 files: both layers get
  `outer_copper_oz`, the core is `board_thickness` minus that copper (rounded to 1 µm), er is the
  FR-4 nominal 4.5 (v1 recorded none). `board_thickness` stays the declared finished thickness;
  `Stackup::validate` (used by `SetStackup` and the load integrity gate) requires the layer
  build-up to match it within ±10 %. DRC-008 sizes each track with its own layer's copper weight.
- **Consequences:** Unequal top/bottom copper weights are now expressible and used. Boards with more
  than two copper layers stay unsupported (option 3 deferred): `validate` rejects any stackup that
  is not exactly top then bottom.
- **Source:** Product-owner decision for the validation boards; Spec §2.1 (one canonical EAK-IR).
