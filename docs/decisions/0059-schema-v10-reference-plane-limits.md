# ADR-0059: Schema v10: the largest return-via distance and the plane neck width
- **Date:** 2026-11-26
- **Status:** Accepted (approved by the first mate for task eak-m76-planes, execution brief §13
  M7.6: the two fields are additive, optional and serde-default, and are set with
  `SetEngineeringInputs`).
- **Context:** M7.6 adds reference-plane checks on the shared board geometry (ADR-0052):
  - DRC-030 asks whether a declared high-speed layer change has a return via of its reference net
    "within a declared distance";
  - the plane connectivity check asks whether a plane zone's fill has a neck narrower than a
    declared width.

  Neither limit is a physical constant. Each is a design choice that depends on edge rates,
  current and the fabricator. Before v10, EAK-IR had no place to declare either one, so each
  check could only report UNKNOWN or guess. `Constraints` and `Command` are shared contracts, so
  the change is recorded here.

## Decision
Two new `eak_core::Constraints` fields. Both are `#[serde(default)]` and left out of the file when
`None`:

| Field | Type | Meaning when absent |
|---|---|---|
| `return_via_max_distance` | `Option<Nm>` | not declared: a return via that is present leaves DRC-030 UNKNOWN, naming the missing distance (never PASS) |
| `plane_min_neck_width` | `Option<Nm>` | not declared: the plane neck check is UNKNOWN, naming the missing width (never PASS) |

- **Command.** `SetEngineeringInputs` gains `return_via_max_distance` and `plane_min_neck_width`
  as `Option<Option<Nm>>`. Unlike the ADR-0055 inputs, an omitted field keeps its value, `null`
  clears it and a number sets it. A caller written before v10 (the desktop, older agents) sends
  the set without these keys, and replacing it must not silently drop limits it never knew about
  (library-lead review of #155). It is one undo step, class `EngineeringImpacting`. The agent tool schema lists both
  fields as nullable nm, so the tool count is unchanged.
- **Validation.** `eak_core::integrity` refuses a declared value that is not positive. The engine
  runs integrity after every apply, so nothing changes, and a file holding such a value fails to
  load.
- **Migration v9 → v10.** Only the version number changes, because absent means undeclared, which
  is exactly the v9 meaning. v9 builds refuse a v10 file; otherwise they would silently drop the
  declared limits and report UNKNOWN against limits the user had set.

## Options considered
- **A fixed built-in distance or width.** Rejected: it would report PASS against a number nobody
  declared, which AGENTS.md forbids.
- **Fields on `DiffPairRule` or `NetClass`.** Rejected for now: the limits are board-wide in the
  M7.6 brief. A per-class override can be added later as another optional field, without changing
  these two.
- **A new command.** Rejected: `SetEngineeringInputs` is already the one command for check inputs
  (ADR-0055).

## Consequences
- DRC-030 compares the distance from each high-speed via to the nearest return via joining both
  reference planes with `return_via_max_distance`. The plane check measures necks against
  `plane_min_neck_width`. Both stay UNKNOWN while their limit is undeclared, and manufacturing
  release stays blocked on them, as for every UNKNOWN rule.
- Limitations: neither limit changes per net class or per layer. The desktop types and dialogs
  for these fields are not part of this record (the desktop IPC surface is out of scope for M7.6).
- **Source requirement:** execution brief §13 M7.6 (validation checks with explicit inputs and
  PASS/FAIL/UNKNOWN).
