# ADR-0023: Routing intelligence in the native router

Status: accepted (2026-11-15), code-reviewed only (never compiled or run)

## Context
Directive 3E asks for congestion analysis, pass-based routing with stagnation detection, selective
rip-up, differential-pair awareness and width/via selection from net class and current, without an
external router and without reducing routing to shortest-path.

## Decision
* `eak_engineering::routing_intel` (congestion map and cost curve, stagnation detector,
  name-based differential pairs, per-net width/via selection from class and IPC-2221 current) and a
  rewritten `router`: priority order, pair partners adjacent with affinity cost, congestion step
  cost, passes with promotion of failed connections, selective rip-up of lower-priority routes
  placed in the same run (restored if it does not help), neckdown to the class width when a wide
  track cannot clear nearby copper. Pre-existing copper is never ripped up.
* `RouteResult` gains fields with `#[serde(default)]` (passes, ripups, stagnated, congestion,
  pairs, notes); `RouterOptions` gains `max_passes`, `patience`, `max_ripups`, `max_victims`.
* New read tools `analyze_routing` (dry run, nothing applied) and `verify_routing` (physical
  connectivity + DRC via eak-validation, with `not_implemented`).

## Consequences
The router reports a connection routed only after exact clearance checks; it certifies nothing.
Pair coupling, length matching, impedance and inner-layer routing are not implemented and are
listed in tool output. Heuristic constants (patience, ripup limits) are unmeasured.

## Addendum 2026-11-26: multilayer routing (workstream A, `fm/eak-route-ml`)
CI-verified on PR #135 (`eak-validation/tests/autoroute_multilayer.rs`); no IR change.

* The grid has one plane per stack-up layer. A* routes on the signal layers only; plane layers
  carry pours (plane nets reach them through `StitchZones`) and are obstacles for vias. A layer
  change is one through via of the net-class size (drill and annular ring raised to the board
  minimums) that must clear every layer it spans; vias exist only at layer changes. Where no
  through via fits, a layer change uses the shortest allowed blind/buried span of
  `Constraints.via_spans` (schema v8, `SetViaSpans`) that joins both layers and clears every layer
  it spans; `RoutedVia` carries its `from`/`to` (serde default top/bottom). Pair members keep
  through vias. A route mixing blind and through vias is spacing-checked at its largest drill.
* Obstacles come from `BoardGeometry` (ADR-0052): copper on every layer, the flattened outline
  and cut-outs, keep-out regions (tracks per layer, vias), hole-to-hole spacing around every drill.
  The exact acceptance check applies the DRC's rules: `clearance::required` (DRC-002), matrix
  cells, DRC-005, DRC-018, DRC-019, DRC-021, DRC-022. `HOLE_TO_HOLE_MIN` moved to
  `eak_engineering::clearance` so the DRC and the router share one value.
* Track width per layer: IPC-2221 external on outer layers, internal on inner signal layers, each at
  the thinnest copper of its group (`NetSpec.inner_track_width`). The whole-connection neck-down
  is removed: it undersized current-carrying tracks (DRC-008 errors). Grid inflation is per layer.
* Net priority adds a high-speed class (name-based pairs and clock names) after power and ground.
* A connection left unrouted is explained on the final board by dropping obstacle classes in turn
  (keep-outs, earlier routes, fixed copper) and naming the objects and rule of the class that
  closes the path. `AutoRoute` ends with a connectivity proof from the board geometry (each net
  one copper group, no shorts, or the open nets and shorts named).
* Not done here: diff-pair coupling, length matching, impedance-aware routing and push-and-shove
  inside the autorouter (sibling workstream `eak-route-dp`); per-net grid inflation (one HV or
  high-current net widens a layer's grid for every net).
