# ADR-0042: Push-and-shove in the route step (engine module, no IR or Command change)
- **Date:** 2026-11-23
- **Context:** Phase 1 slice C3 (plan gaps #27, #28). The interactive router
  (`eak_engineering::interactive::route_step`, desktop IPC `route_step`) could only walk around
  other-net copper or stop before it. The EasyEDA behaviour study lists push (shove) as a
  routing conflict mode users expect, and the brief asks for push-and-shove of non-locked tracks
  behind a mode switch, never creating a DRC-002 (clearance) or DRC-003 (short) violation. The
  brief also fixes the scope: an engine change in the routing module, not EAK-IR, and no new
  `Command` (slice C4 owns IR and `Command`). `RouteStepRequest`/`RouteStepResult` are the
  desktop IPC's request and report, so this ADR records their extension.
- **Options considered:**
  1. A new `Command` (for example `ShoveTracks` or `MoveTrack` with a polyline) applied at each
     route step. Rejected for this slice: `Command` belongs to C4, and a committed change per
     step would make one route many undo steps.
  2. Keep the route step stateless and make it return proposals only; the route tool commits
     the moved tracks with the route as existing commands. **Chosen.**
  3. A full multi-level shove (moved tracks pushing third tracks, springback). Deferred: one
     level covers the common "route beside a parallel track" case and is checkable exactly.
- **Decision:**
  - `ConflictMode` gains `push` (serde `"push"`). In push mode, before the walkaround, the step
    tries each direct path; when only other-net **tracks** block it, each blocking track is moved
    sideways, away from the route, by a 45-degree bump that keeps both of its ends (so its
    connections are kept). Pads, vias, zones and the board edge are never moved: if one of them
    blocks, the step walks around as in `walkaround`. A track is refused (walkaround instead)
    when the route touches or crosses its line, the bump would reach one of its ends, or copper
    of its own net joins it inside the bump.
  - Every moved track is re-checked exactly at its new place against all other-net copper, the
    route, this step's via, the other moved tracks and the board-edge clearance, with the
    DRC's clearance rule (the larger of the two net classes' clearances and the board minimum)
    plus a 2 µm margin for rounding. Any failure drops the push for that path.
  - `RouteStepResult.shoved` lists the moved tracks as full replacement polylines
    (`ShovedTrack { id, layer, width, net, points }`; first and last points are the track's own
    ends). `RouteStepRequest.pending` carries the route drawn so far but not committed (its
    fixed segments, vias and the latest move of each track), so the next step sees that board.
    A track is always bumped from its committed place against the whole route so far, so one
    route makes one bump per track. Both fields are `#[serde(default)]`: old callers are
    unchanged.
  - The route tool (`routeTool.ts`) keeps each step's `shoved`, sends `pending`, and commits the
    finished route as one `Batch`: `DeleteTrack` of each moved track, `AddTrack` for each
    segment of its polyline, then the route's own `AddVia`/`AddTrack`. One revision, one undo
    step; nothing writes project files directly.
- **Consequences:** push mode is single-level and shove-only (no springback, no via shove). The
  EAK-IR has no track lock, so every track can be pushed; a lock needs an IR field (C4).
  Voltage-based clearance (DRC's spacing by net voltage) is not considered by the shove; DRC
  still reports it. A moved track becomes several tracks with new ids (the IR has no polyline
  track). Tests: `interactive.rs` push unit tests, the DRC property test
  `crates/eak-validation/tests/push_shove.rs` (generated boards, the committed result has no
  DRC-002/003), `routeTool.test.ts` (pending, Batch), CI ui-evidence `c3-push-offered`.
- **Source requirement:** product spec, interactive routing (walkaround, push-and-shove, live
  clearance); Phase 1 plan gaps #27, #28.
