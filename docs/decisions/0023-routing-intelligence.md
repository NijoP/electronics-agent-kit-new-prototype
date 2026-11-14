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
