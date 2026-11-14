# ADR-0014: Library part properties and footprint assignment commands

Status: accepted (Phase 5, library panel)

## Context
The desktop library panel must let a user edit supplier-facing properties (manufacturer, MPN,
LCSC id, datasheet) of a part and assign a different footprint. Both could be expressed by
`UpsertComponentDefinition`, but that makes the UI (and the AI) resend a whole definition for a
one-field edit, which is error-prone and hides intent in the history.

## Decision
Two new typed `Command` variants, no IR/schema change (`ComponentDef` already holds the fields):
- `SetComponentProperties { component_id, manufacturer, mpn, lcsc, datasheet }` (reversible local).
  `None` or a blank string clears a field. Provenance, ratings, symbol, footprint and pin map are
  never touched.
- `AssignComponentFootprint { component_id, footprint, pin_map }` (engineering-impacting). The
  resulting definition must pass `ComponentDef::validate`; the change is refused while PCB
  footprint instances of the component exist (their pad nets would silently go stale); the previous
  provenance text is kept and a "footprint reassigned to <name>" note appended.

Both operate on the project library only. A built-in part is copied with `ImportLibraryComponent`
(or implicitly by `PlaceSymbol`) first. Undo is the engine's generic snapshot undo.

Read-only IPC `library_search` and `library_footprint_candidates` merge the built-in catalogue with
the project library (project shadows built-in) and offer only footprints that pass validation.
Search logic lives in `eak_core::ComponentDef::matches_query`.

## Consequences / limitations
- No licence field exists in the library model; the panel says so rather than inventing one. Adding
  `ComponentDef.license` and per-field evidence is a schema bump needing its own ADR (plan Phase 5).
- Supplier (LCSC) search is not duplicated here: it lives in the supplier-integration work (PR #17).
