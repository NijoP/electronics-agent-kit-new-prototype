# ADR-0045: Object groups (schema v6), Group / Ungroup and RenameNetLabel
- **Date:** 2026-11-23
- **Context:** Follow-ups reported by Phase 1 slice B3 (schematic editing set). The Group action
  was listed but unavailable, because EAK-IR had no group object. Find and replace, and the
  Inspector's net-name field, renamed a net label by deleting it and adding a new one in one
  `Batch`. The label then got a new id, so selections, history and cross-references to the old id
  broke. The PCB and the constraints also kept the old net name until the next Update PCB.
- **Options considered:**
  1. Groups as editor-only state (desktop memory). Rejected: they would be lost on reload, could
     not be undone, and AI and CLI could not use them. Every design change goes through a typed
     `Command` (AGENTS.md).
  2. A `group_id` on each symbol, wire and label. Rejected: three IR types would change, and a
     group's name would need a home anyway.
  3. **Chosen:** a keyed collection `Schematic.groups: BTreeMap<Id, SchematicGroup>`. A group has
     `id`, `sheet_id`, `name` and `members` (sorted ids of symbols, wires and net labels). The
     existing keyed-collection undo delta covers it (`delta.rs` lists `schematic.groups`), so
     Group and Ungroup are one revision and one undo step each.
  4. Net-label rename as `DeleteNetLabel` + `AddNetLabel` (the B3 workaround). Rejected, because
     the label gets a new id. **Chosen:** `RenameNetLabel` changes the name in place. It uses the
     same net-rename path as `RenameSheet` (ADR-0043): it builds the netlist before and after,
     matches nets by pin (or else by label), and rewrites the PCB and constraint references of
     every renamed net in the same transaction.
- **Decision:**
  - IR schema **v6**: `Schematic.groups` (always written, so undo deltas have a path to restore
    into). Migration v5 → v6 adds the empty collection. Nothing else changes, so a v5 file has the
    same meaning. Integrity: key = id, the sheet exists, the name is not blank, there are at least
    `MIN_GROUP_MEMBERS` (2) members, every member exists on the group's sheet, and an object is in
    at most one group.
  - `Group { members, name? }` (ReversibleLocal): sorts the members and removes duplicates. It
    refuses fewer than 2 objects, ids that are not a symbol, wire or net label, objects on
    different sheets, and an object that is already in a group. The default name is the first free
    `Group N`.
  - `Ungroup { group_id }` (ReversibleLocal): removes the group. Its objects stay where they are.
  - After every applied command, `apply` prunes groups (`Schematic::prune_groups`). Members that no
    longer exist are dropped, and a group left with fewer than 2 members is dissolved, with a
    warning. Deleting a grouped object (or emptying its sheet) therefore changes its group in the
    same undo step. Integrity is never violated.
  - `RenameNetLabel { label_id, name }` (EngineeringImpacting): trims the name. It refuses a blank
    name, an unchanged name (nothing to do) and an unknown label. It warns with the renamed nets
    (`A → B`). It also warns when the new name already named a net, because the rename then joins
    those nets.
  - Grouping is an editing aid only. Netlist, ERC, DRC and every export ignore groups.
  - The tool schemas (85 commands), the harness phase list and the task-invalidation map name the
    three commands. A rename invalidates nets; grouping invalidates nothing.
  - Desktop: clicking a grouped object selects its whole group, so move, copy and delete act on
    the group. Edit > Group and Ungroup, and the canvas context menu entries, come from the action
    registry. The Inspector shows a Group row (group or ungroup the selection) and renames labels
    with `RenameNetLabel`. Find and replace renames labels with `RenameNetLabel` too.
- **Consequences:** Pasted copies are not grouped (the clipboard holds objects, not groups).
  Groups hold schematic symbols, wires and net labels only. Buses, drawings and PCB objects
  cannot be grouped yet, and the Group action says so by not offering them. Tests: `eak-core`
  `migrate` tests (`v6_groups_round_trip_byte_stable`,
  `v5_project_migrates_to_v6_with_identical_meaning`, `invalid_groups_fail_integrity`,
  `prune_groups_…`), `crates/eak-runtime/tests/groups.rs`, the desktop vitest suites and the
  ui-evidence scenario `apps/desktop/e2e/scenarios/b3-groups.json`.
- **Source requirement:** Product spec EDA workspace (schematic editing); Phase 1 plan B3
  follow-ups.
