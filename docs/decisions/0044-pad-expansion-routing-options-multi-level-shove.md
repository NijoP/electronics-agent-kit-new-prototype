# ADR-0044: Schema v5 — per-pad mask/paste expansion, saved routing options; multi-level, voltage-aware push
- **Date:** 2026-11-23
- **Context:** Follow-ups reported by finished Phase 1 slices. D2's footprint editor said
  "Solder-mask and paste expansion per pad: not available yet": the EAK-IR had only the board-wide
  `ManufacturingConfig::mask_expansion` and no paste value at all, so a footprint could not carry a
  fabricator- or datasheet-specific opening. C3's routing options (corners, obstacle mode, width,
  via, loop removal) lived in React state and were lost when the project closed. C3's push-and-shove
  (ADR-0042, route step) was one level only (a moved track never moved a third one) and used the
  net-class clearance, while the DRC (DRC-002) also requires the IPC-2221B spacing for declared net
  voltages, so a push could propose a board the DRC then reported. The rules worker (slices B5/C5)
  owns rule and constraint types; this change adds no rule type and reads the existing
  `Constraints::net_voltages_v` and net classes.
- **Options considered:**
  1. Per-pad expansion as a footprint-level override map (`pad -> expansion`). Rejected: a second
     place for pad data; the pad is the natural owner (KiCad and EasyEDA both store it per pad).
  2. **Chosen:** two optional fields on `PadDef`, `mask_expansion` and `paste_expansion`
     (`Option<Nm>`, `None` = board default / no paste change, omitted from the file when `None`).
  3. Routing options in desktop local storage. Rejected: per machine, not per board, invisible to
     the CLI and the AI, and not undoable.
  4. **Chosen:** `Pcb.routing: RoutingOptions` in the EAK-IR, written only by a new command.
  5. Push depth by springback/iterative relaxation (full PNS). Deferred: an exact, checkable
     bounded chain covers the common "parallel bus beside the route" case.
- **Decision:**
  - **Schema v5.** `SCHEMA_VERSION = 5`. `migrate.rs` v4 → v5 adds `pcb.routing` with its defaults
    (45-degree corners, walkaround, net-class width and via, loop removal on); a v4 pad has no
    expansion of its own, which is the v4 meaning. Older builds refuse a v5 file (unsupported
    schema) instead of silently dropping the new fields on save.
  - **IR.** `PadDef.mask_expansion`, `PadDef.paste_expansion` (per side; negative shrinks the
    opening; at most 1 mm either way; the opening must stay open; paste is for SMD pads only),
    checked by `PadDef::expansion_problems` inside `ComponentDef::validate` (so integrity on load and
    every library command enforce it). `BendMode`, `ConflictMode` and `ViaSize` move from
    `eak_engineering::interactive` into `eak-core` (re-exported under their old paths, same serde
    names) so `RoutingOptions { bend, conflict, width, via, remove_loops }` can live in the IR;
    `RoutingOptions::problem` (positive width, diameter > drill > 0) is checked on load.
  - **Commands** (82 in all). `SetPadExpansion { component_id, pad, mask_expansion,
    paste_expansion }` (engineering-impacting; allowed while the part is placed, because openings
    change and copper and connectivity do not; `AssignComponentFootprint` stays refused then).
    `SetRoutingOptions { options }` (reversible-local; changes no copper). Both are one revision
    and one undo step, have typed AI tool schemas, and `task_invalidate` treats `SetPadExpansion`
    like `SetManufacturing` (constraints changed).
  - **Outputs.** The soldermask Gerber and `silk::mask_openings` (DRC-015's model, so the legend is
    clipped to the same openings) use each pad's own mask expansion, else the board-wide one. The
    paste Gerber sizes each SMD aperture by the pad's paste expansion.
  - **Shared clearance rule.** New `eak_engineering::clearance::required(c, a, b, layer)`: the board
    minimum, both net classes and, when both nets declare a voltage, the IPC-2221B Table 6-1 spacing
    for their difference (B2 outer, B1 inner; table unverified). The DRC's DRC-002 voltage part now
    calls the same `voltage_clearance`, and the route step uses `required` for every obstacle
    (direct paths, walkaround grid, live feedback `required`) and for every push check.
    `RouteStepResult.clearance` stays the route net's own class clearance.
  - **Multi-level push.** In `push` mode a track moved out of the route's way that then comes closer
    than the rule to another other-net track pushes it too, bumped from its committed place against
    the route and every track moved at an earlier level, up to `MAX_SHOVE_LEVELS = 3` levels and 24
    tracks per step. Arcs, pads, vias, zones, the board edge and the route's own net never move;
    anything that would need a fourth level, or any remaining violation in the exact re-check of all
    moved tracks, drops the push and the step walks around instead. The route tool already commits
    every moved track (`RouteStepResult.shoved`) in the route's one `Batch`.
  - **Desktop.** The footprint editor's pad table gets Mask and Paste columns (empty = default);
    a draft that changes only those saves as per-pad `SetPadExpansion` (allowed while placed),
    anything else as `AssignComponentFootprint` carrying the fields. The route tool's inspector
    starts from `pcb.routing`, follows it when it changes (open, undo), and saves a change with
    `SetRoutingOptions` after a 600 ms pause (typing a width is one command, not one per key).
- **Consequences:** v5 files do not open in v4 builds. Saving a routing option is an undo step like
  any other edit. Push is still shove-only (no springback, no via shove, no walkaround of a moved
  track) and three levels deep; the DRC remains the authority. Creepage and paste-ratio checks are
  still `not_implemented` (DFM). Tests: `eak-core` `migrate.rs` (v5 round trip, v4 → v5,
  integrity refusals), `eak-engineering` `clearance.rs` and `interactive.rs` (second-level push,
  level limit, voltage clearance in walkaround and push), `eak-validation/tests/push_shove.rs`
  (multi-level, voltage-aware property test against the DRC), `eak-manufacturing` `gerber.rs`
  (per-pad mask and paste apertures), `eak-runtime/tests/ir_v5.rs` (both commands, undo, refusals,
  save/load), `eak-intelligence` tool schemas, desktop `partEditor.test.tsx`, `routeTool.test.ts`,
  `inspector.test.tsx`; CI ui-evidence `v5-*` steps in `c3-routing.json` and `d2-editors.json`.
- **Source requirement:** product spec, footprint editor (pad properties incl. mask/paste),
  interactive routing (push-and-shove, live clearance, persisted options); Phase 1 follow-ups of
  slices C3 and D2.
