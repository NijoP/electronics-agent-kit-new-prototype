# ADR-0048: SetNetLabelKind (change a label's kind in place)
- **Date:** 2026-11-23
- **Context:** Phase 1 definition-of-done audit, defect S5 (fix F3). The Inspector changed a
  label between a net label and a power port by deleting it and adding a new one in one `Batch`.
  The label then got a new id (selections, history and references to the old id broke), it left
  its group (ADR-0045), and its power-port style was lost. `RenameNetLabel` (ADR-0045) already
  replaced the same workaround for the name.
- **Options considered:**
  1. Keep Delete + Add and carry the fields through the `Batch`. Rejected: `AddNetLabel` always
     makes a new id, and the group membership would need a third command (`Group` again) that can
     fail if the group was dissolved by the delete in the same step.
  2. **Chosen:** a typed `SetNetLabelKind { label_id, kind }` that changes only `kind`.
- **Decision:**
  - `SetNetLabelKind` (EngineeringImpacting, like `SetNetLabelScope`) changes the label's kind
    and nothing else: id, name, position, scope, power style and group stay. A power style on a
    net label is kept but not drawn (the renderer draws it for power ports only), so changing a
    port to a net label and back restores its look. It refuses an unknown label and an unchanged
    kind (`NothingTo`). It warns when the label is local, because a power port connects on every
    sheet whatever its scope, so the change can split or join nets (run ERC).
  - No IR change: `NetLabel.kind` already exists, so there is no schema bump and no migration.
    The keyed-collection undo delta covers the label, so the change is one revision and one undo
    step; several labels go in one `Batch`.
  - The tool schemas (89 commands), the harness phase list and the task-invalidation map
    (`Change::Net`) name the command. The desktop Inspector's Kind field sends it.
- **Consequences:** Net names on the PCB are not rewritten by a kind change (the label name is
  unchanged). Tests: `crates/eak-runtime/tests/label_kind.rs`, the `eak-intelligence` tool-schema
  count, and the desktop vitest `inspector.test.tsx`.
- **Source requirement:** Product spec EDA workspace (schematic editing); Phase 1 DoD audit S5.
