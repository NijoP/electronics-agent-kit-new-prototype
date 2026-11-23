# ADR-0041: EAK-IR schema v4, PCB objects (keepout and constraint regions, text, dimensions, arcs, cut-outs) and PCB edit commands
- **Date:** 2026-11-23
- **Context:** Phase 1 slice C4 (plan gaps #30-#32). Schema v3 could not edit a track, via or zone
  after placing it (delete and redraw only), could not delete a footprint, and had no keep-out
  area (DRC listed "Keepout areas: EAK-IR has no keepout objects yet" in `not_implemented`), no
  PCB text or dimension, no arc track or arc board edge and no board cut-out. This changes shared
  contracts (IR types, `Command`, the DRC report and the Gerber output), so it needs an ADR and a
  migration (AGENTS.md). Semantics follow the clean-room EasyEDA behaviour notes (regions §3.4,
  object fields §4 PCB, "dragging vias keeps tracks attached" §3.1), adapted to EAK.
- **Options considered:**
  1. Arc tracks as a separate collection (EasyEDA keeps ARC records apart from LINE records).
     Rejected: every copper consumer (clearance, shorts, connectivity, dangling ends, width and
     current rules, layer rules, Gerber, renderers) would have to read two collections for one
     kind of conductor.
  2. Replace the board outline polygon with a path type. Rejected for this slice: the placer, the
     router, the zone fill, DRC, DFM, the 3D scene and both desktop renderers read the polygon.
  3. Add the new objects to the existing IR with defaults that reproduce v3 exactly, an optional
     sweep angle on tracks and on outline edges, flattened (chord error at most 10 µm) wherever
     a polygon or segment is needed and written as true arcs in Gerber. One migration step.
     **Chosen.**
- **Decision (IR, `eak-core`, new module `pcb_objects.rs`):**
  - Arc geometry: an arc runs from `a` to `b` and sweeps `sweep_deg` degrees, positive
    counter-clockwise (Y up), `0 < |sweep| < 360`. `arc_center`, `arc_radius`, `arc_points`
    (flattened, chord error ≤ `ARC_FLATTEN_ERR_NM` = 10 µm, end points exact) and `arc_mid`.
  - `Track.arc_deg: Option<f64>`: `None` (every v3 track, omitted from the file) is a straight
    segment; a value makes the track an arc of that sweep from `a` to `b`. `Track::path()` gives
    the flattened centre line and `Track::length()` the true length.
  - `Board.outline_arcs: Vec<OutlineArc { edge, sweep_deg }>`: edge `i` runs from `outline[i]`
    to `outline[i + 1]` (the last edge closes the polygon). Omitted when empty.
    `Board::outline_path()` is the flattened outline; DRC, the zone fill and the hole and
    silkscreen checks read it, the Gerber profile writes true arcs. The placer, the router and the
    renderers' board fill still read the vertex polygon (the chord); this is listed as a DRC
    limitation.
  - `Pcb.cutouts: BTreeMap<Id, Cutout { id, outline }>`: holes milled through the board. Copper
    inside a cut-out or within the board-edge clearance of its edge is a DRC-005 error; the zone
    fill leaves cut-outs empty; Gerber writes them on the profile layer.
  - `Pcb.regions: BTreeMap<Id, Region>`: `{ id, name, outline, layers, forbid, min_track_width,
    max_track_width }`. `layers` empty = every copper layer. `forbid` is a set of `tracks`, `vias`,
    `components` (footprint pads and courtyards) and `zones` (pour fill); a region with a
    non-empty `forbid` is a keep-out. `min_track_width` / `max_track_width` make it a constraint
    region for the tracks inside it. A region needs a keep-out flag or a width rule.
  - `Pcb.texts: BTreeMap<Id, PcbText { id, layer, position, text, size, stroke_width, rotation }>`
    and `Pcb.dimensions: BTreeMap<Id, Dimension { id, layer, a, b, offset, unit, precision,
    text_size }>`. `layer` is a `DrawLayer`: `top_silk`, `bottom_silk` or `documentation`. Text is
    drawn with the built-in stroke font, anchored at the baseline centre, mirrored on the bottom
    silk. A dimension is an aligned linear dimension of `a`-`b`, its line offset by `offset`
    (positive = left of `a`→`b`), labelled with the measured length in `mm`, `mil` or `inch` at
    `precision` decimals (0-6). Silk text and dimensions are part of the legend
    (`eak_engineering::silk::raw_legend`), so DRC-015/DRC-020 check them and the legend Gerber
    prints them; documentation-layer objects are drawings only and are not fabricated.
  - Integrity checks the new objects (ids, simple polygons, sweeps, sizes).
- **Decision (`Command`, `eak-runtime`):** 15 new variants, 76 commands in total.
  - `MoveTrack { track_id, by }` moves a segment and stretches the ends of same-net tracks on the
    same layer that met its old ends (drag segment). `EditTrack { track_id, width, layer, a, b,
    arc_deg }` changes any of these; `arc_deg: 0` makes the track straight.
  - `MoveVia { via_id, position }` moves a via and drags the ends of same-net tracks on the layers
    it spans that ended on it.
  - `EditZone { zone_id, outline, net, layer, clearance }`.
  - `DeleteFootprint { footprint_id }` (refused while locked; tracks stay and DRC reports them; a
    later `SyncSchematicToPcb` re-creates the footprint while its symbol exists, and the response
    warns about that).
  - `AddRegion` / `DeleteRegion`, `AddPcbText` / `MovePcbText` / `DeletePcbText`, `AddDimension` /
    `DeleteDimension`, `AddBoardCutout` / `DeleteBoardCutout`, `SetOutlineArc { edge, sweep_deg }`
    (`null` or 0 makes the edge straight). `AddTrack` takes an optional `arc_deg`.
    `SetBoardOutline` drops outline arcs (with a warning) because edge indices change meaning.
  - Classes: text, dimensions and `MovePcbText` are `ReversibleLocal`; everything else here is
    `EngineeringImpacting` (copper, connectivity, rules or the board shape change). Undo/redo
    deltas cover the four new collections. The agent tools, the harness routing/placement command
    lists and the task-invalidation map (`Change::BoardOutline` for cut-outs and outline arcs,
    `Change::Rules` for regions, `Change::Footprint` for `DeleteFootprint`) include them.
- **Decision (DRC, `eak-validation`):** DRC-021 keep-out: a forbidden track, via, footprint pad,
  or zone outline inside or crossing a keep-out on a layer it applies to (error). DRC-022
  constraint region: a track inside a region narrower than its `min_track_width` or wider than its
  `max_track_width` (error). DRC-023 drawing layer: silk text or a dimension label smaller than the
  0.8 mm legibility minimum, or a character the stroke font cannot draw (warning). DRC-005 also
  covers cut-outs and arc outline edges. The keep-out line leaves `not_implemented`; region
  clearance rules, keep-outs on silk or mask layers and region rule priority are listed as not
  implemented. The zone fill honours `zones` keep-outs and cut-outs.
- **Decision (Gerber, `eak-manufacturing`):** `G75` multi-quadrant mode is declared once; arc
  tracks and arc outline edges are written as `G02`/`G03` circular interpolation with `I`/`J`
  centre offsets; cut-outs are closed contours on the profile layer; silk text and silk dimensions
  are stroked on the legend layers. `gerber::validate` checks arcs structurally: an arc needs
  `G75`, a current point and `G02`/`G03` mode, and its start and end must be equidistant from the
  centre (within 1 µm).
- **Migration (`migrate.rs`, v3 → v4):** adds the empty `pcb.regions`, `pcb.texts`,
  `pcb.dimensions` and `pcb.cutouts` collections. Nothing else changes: tracks load straight, the
  outline without arcs. The migrated file is byte-stable on re-save and has the identical DRC
  findings.
- **Consequences:** v4 files are refused by older builds (as for v3). The desktop PCB view draws
  regions, cut-outs, text, dimensions and arc tracks/edges in both renderers, and the Delete tool
  deletes a footprint; creating and editing the new objects is reachable through commands (CLI,
  MCP, agent). Interactive drawing tools for them are later UI work and no desktop control claims
  them. The autorouter does not avoid keep-outs or cut-outs yet (DRC reports what it routes into
  them); this is listed as a DRC limitation.
- **Source requirement:** product spec, PCB layout (tracks, vias, zones, keep-outs, board shape,
  text and dimensions, Gerber output); Phase 1 plan slice C4 (gaps #30-#32).
