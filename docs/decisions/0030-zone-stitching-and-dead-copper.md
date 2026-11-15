# ADR-0030: Zone stitching command and dead-copper removal (DESK pour method)

Status: accepted (code-reviewed only, never compiled)

## Context
Dev-board run B (`docs/qa/DEVBOARD_E2E.md`) ended with GND unrouted and a GND pour split into 43
islands (`copper_continuity` Fail). EAK had a single pour per layer and no way to join SMD ground
pads to a pour on another layer or to anchor split islands. DESK (`tools/copper_fill.py`,
`tools/stitch_orphans.py`, MIT) pours GND on every layer, drops fill islands no via or pad
anchors, and places escape vias for orphan GND SMD pads and one via per orphan island. See
`docs/engineering/DESK_PIPELINE_PARITY.md`.

## Decision
- New typed command `StitchZones { net: Option<String> }` (EngineeringImpacting, reversible by
  Undo). It runs `eak_engineering::stitch::plan` and inserts ordinary `Track`/`Via` records:
  per SMD pad of the zone net not reached by its own layer's fill, a ring-searched escape via
  (0.6-3.0 mm, 24 angles) plus pad-to-via track; per fill island with no via or THT pad, one via
  at its deepest cell that lands in a connected same-net fill on another layer. Every via/track is
  checked with exact geometry against other-net copper at the net-class clearance; vias never
  touch pads. Needs a zone of the net on two or more layers, otherwise it reports why and places
  nothing (`nothing_to_do`).
- `copper::fill_zone` (used by DRC, connectivity, gates, exports and the UI) now drops islands
  that touch no copper of the zone net on their layer (`ZoneFill::removed_islands`); the raw fill
  is `fill_zone_raw`. DRC-013 reports a zone whose whole fill was dropped.
- AutoRoute: when every attempted connection fails it returns `precondition` with the reasons
  (previously the empty change surfaced as a bare "no change"); `nothing_to_do` lists the nets that
  are still unrouted.
- ERC-003's fix names the exact `LabelPin … "kind":"power"` command on the connector pin where the
  rail enters (power port = PWR_FLAG equivalent). Pin types and the rule itself are unchanged.

## Consequences
Shared contract: one `Command` variant (Rust, TypeScript `types.ts`, tool schema, rework guard
requires `net`). Fabricated copper changes: dead islands are no longer emitted. Stitching is a
heuristic; connectivity is still judged only by `eak-validation`. Not verified at runtime.
