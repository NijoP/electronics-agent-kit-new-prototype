# ADR-0052: One derived board geometry for DRC, connectivity, 2D and 3D
- **Date:** 2026-11-26
- **Context:** EAK v2 slice E1 (board geometry truth). Each consumer re-derived the board on its
  own: DRC called `copper::items` and `copper::fill_zone`, then `copper::analyse` filled every zone
  again; the `connectivity` IPC filled them a third time for `zone_fills`; `holes` and the silk
  checks each re-walked the IR. The 3D view built its scene in TypeScript from raw IR (arc tracks
  drawn straight, outline arcs and cut-outs ignored, no zone fill, courtyards as silk), and
  `scene3d::build` read the raw outline and was called by nothing. Agreement between what is
  checked and what is drawn rested on determinism, not construction.
- **Options considered:**
  1. Keep per-consumer derivation and add cross-checks. Rejected: N copies to keep aligned, and
     the 3D gaps stay.
  2. Store derived geometry in the IR. Rejected (as in ADR-0047): stale-able second copy, schema
     bump, and DRC would still have to recompute it.
  3. **Chosen:** `eak_engineering::board::BoardGeometry::build(p)`, derived on request: flattened
     outline, cut-outs, thickness, every copper item (`CopperItem`), zone fills after clearance,
     keep-outs, thermal reliefs and dead-island removal (`ZoneFill`, plus `FillView` rectangles of
     the very same fills), drilled holes (`Hole`, moved from `eak-validation`) and the mask-clipped
     legend per side.
- **Decision:** `drc::run(p)` is `drc::run_with(p, &BoardGeometry::build(p))`; every copper, fill,
  hole and legend check, and DRC-004 connectivity (`copper::analyse_with`), read that value.
  `holes::copper_to_hole`, `hole_to_hole` and `silk_to_edge` take the geometry (DFM passes its
  hole list). `scene3d::build(p, &g)` takes its outline and thickness from it. The desktop reads
  the same value through a read-only `board_geometry` IPC (serialized without the raw raster
  cells). No IR change, no new command, no new dependency.
- **Consequences:** one fill per zone per DRC run instead of two; the regression test
  `eak-validation/tests/board_geometry.rs` proves, across a zone-clearance edit that moves a
  thermal spoke, that DRC's relief state equals the copper in the serialized payload. Gerber still
  derives through the same `copper`/`silk` functions directly (not yet through `BoardGeometry`).
