# ADR-0047: Zone fill is derived and drawn from the DRC raster; stack-up edited through SetStackup
- **Date:** 2026-11-23
- **Context:** Phase 1 slice E5 (planes and stack-up completion). The PCB editor drew each zone as
  its translucent outline polygon, so the user never saw the copper the board would actually get:
  no clearance cut-outs, thermal reliefs or removed dead islands. DRC, connectivity and stitching
  already compute that copper with `eak_engineering::copper::fill_zone` (0.1 mm raster, AD-6).
  The Layers panel showed copper weight, dielectric gaps, εr and board thickness as read-only, and
  a plane layer had no way to say which net it carries.
- **Options considered:**
  1. Store a fill cache in the IR (per-zone polygons, refreshed by a `RefillZones` command).
     Rejected: a second copy of geometry the engine can recompute, which goes stale whenever a pad,
     track, rule or outline moves; it would need a schema bump, a migration and invalidation rules,
     and DRC would still have to recompute it to be trustworthy.
  2. A separate fill algorithm in the UI (polygon clipping in TypeScript). Rejected: a second
     copper model that could disagree with DRC.
  3. **Chosen:** derive the fill on request from the IR with the same `fill_zone` DRC uses and send
     it to the UI as rectangles (`eak_engineering::fill_view`). Nothing is stored; no IR change.
  4. Board thickness through `SetLayerCount { board_thickness }`. Rejected for the editor: it
     rebuilds the default build-up and discards edited copper weights, gaps and layer kinds.
     **Chosen:** the new thickness scales every dielectric gap by one factor (copper kept) and is
     sent as `SetStackup`, which `Stackup::validate` checks (±10 % build-up tolerance).
- **Decision:**
  - `fill_view::fill_views(p)` returns, per zone, the cells `fill_zone` leaves filled merged into
    axis-aligned `[x0, y0, x1, y1]` nm rectangles that tile them exactly (no overlap, no gap), with
    islands, removed dead islands and filled area. The desktop `connectivity` IPC returns them as
    `zone_fills` next to the ratsnest, for the same revision. The Canvas2D and SVG renderers draw the
    zone outline plus these rectangles; before the fill of the current revision is known, the zone
    area is drawn translucent as before. Outline view draws the outline only.
  - The Layers panel edits copper weight, dielectric gap, εr, finished thickness and dielectric
    material as `SetStackup` commands; the engine validates every one and refuses an inconsistent
    build-up with its reason.
  - Plane-net assignment is not a new IR field: a plane layer's net is the net of its zones. Choosing
    a net moves the layer's zones to it (`EditZone`, one `Batch`) or, with no zone yet, adds one over
    the board outline (`AddZone`).
  - `StitchZones` is reachable from the Design menu (all nets) and per net in the Layers panel; both
    are disabled with the reason until a net is poured on two or more copper layers.
- **Consequences:** No schema change, no migration. The fill is recomputed on every revision that
  touches the PCB (the cost of `fill_zone` per zone, already paid by DRC); a very large board pays it
  once more per revision for drawing. Fill method, island and priority settings remain unavailable:
  the IR has no fields for them. The fill is a 0.1 mm raster, so drawn edges are stepped at that
  pitch, exactly as DRC sees them.
- **Source requirement:** product spec AD-6 (zone fill computed in Rust); approved UI inventory row
  E5 ("Fill equals DRC copper model; stack-up round-trip").
