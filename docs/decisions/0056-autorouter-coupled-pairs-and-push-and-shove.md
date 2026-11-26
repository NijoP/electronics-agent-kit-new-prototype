# ADR-0056: Autorouter: coupled differential pairs and push-and-shove of existing tracks
- **Date:** 2026-11-26
- **Status:** Accepted (workstream A, eak-route-dp; builds on the schema v8 inputs of ADR-0055 and on
  the multilayer router of the ADR-0023 addendum). Landed in increments: coupled pairs first, then
  length matching, then push-and-shove.
- **Context:** The autorouter routed pair members as two single nets that were only steered
  near each other, and it never touched copper that was already on the board. A connection
  blocked by a hand-routed, lower-priority track stayed unrouted, because rip-up only removes
  routes placed during the same pass. `RouteResult` and `PairReport` are report types that the
  `AutoRoute` command, the agent tools and the desktop read, so the change is recorded here.

## Decision
- **Coupled pairs** (`router::diffpair`): a declared pair (`Constraints::diff_pairs`) is routed
  as one centreline.
  - **Grid:** the centreline is routed on a grid where both members count as one net and other
    copper is grown by the member offset.
  - **Ends:** each end breaks out perpendicular to its pad axis, so each member reaches its own
    pad without running over its partner's.
  - **Members:** the two members are the mitred offsets of the centreline. At a layer change
    they spread to a via pitch that keeps the via-to-via clearance.
  - **Width and gap:** they come from the rule. When no gap is declared it is derived from
    `target_zdiff_ohm` with `impedance::gap_for` (IPC-2141A). If it cannot be derived, the pair
    is not routed coupled and the missing input is named.
  - **Length matching:** when the skew exceeds `max_skew`, the shorter member gets an
    exact-length serpentine on its outer side.
  - **Checks:** every member is checked exactly against all copper and against its partner.
  - **Report:** `PairReport` gains `declared`, `coupled`, `width_mm`, `gap_mm`, `max_skew_mm`,
    `skew_within_tolerance` and a per-layer `impedance` estimate with
    `impedance_within_tolerance`. `None` means UNKNOWN, never a pass. All new fields are serde
    defaults.
  - **Rip-up:** coupled copper is never ripped up alone.
- **Push-and-shove in `AutoRoute`:**
  - **When:** a connection that finds no path is tried again as if the pre-existing tracks of
    lower-priority nets were absent. Priority is `priority::net_priority`, and a declared
    pair's tracks are never moved.
  - **How a track moves:** each crowded track is moved with the route tool's 45-degree bump
    that keeps both of its ends (`interactive::bump`, ADR-0042/0044). A moved track pushes the
    next one, up to three levels and 24 tracks.
  - **Acceptance:** the result is accepted only if the route and every moved track pass the
    exact clearance check (the DRC-002 rule per net pair) and the board-edge clearance.
  - **Fallback:** otherwise the router falls back to the bounded rip-up of routes placed in
    the same pass.
  - **Result:** `RouteResult.replaced_tracks` lists the moved track ids, and their new
    polylines are among `tracks`. `AutoRoute` deletes those ids and adds the new tracks in its
    one undo step, and it warns how many tracks it pushed aside.
- The exact clearance check of every route now applies the DRC-002 rule for each pair of nets
  (classes, net pairs, voltage), not only the routed net's class clearance.

## Consequences
- `AutoRoute` can now change copper the user drew. It does so only for lower-priority nets,
  keeps the moved track's ends and connectivity, says so in a warning, and is undone in one
  step. There is still no track lock in the IR, so a user cannot pin a track against a shove.
- **Limitations:**
  - Vias and pads are never shoved.
  - A pair whose centreline changes layer right at an end, or turns more than 135 degrees, is
    routed as two single nets, and the note says why.
  - Serpentines are not checked against a minimum tuning-bump spacing beyond clearance.
  - The impedance figures are closed-form IPC-2141A estimates, not a field solver.
