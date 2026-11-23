# ADR-0043: Sheet management — RenameSheet, DeleteSheet, ReorderSheets and a sheet order
- **Date:** 2026-11-23
- **Context:** Phase 1 slice B6 (plan gap #22). A project could gain sheets (`AddSheet`) but never
  rename, delete or reorder them. The Pages tab of the design manager listed them read-only, and
  the order was the order of the sheet ids (`BTreeMap` keys). That order drives the title-block
  sheet number, the schematic PDF page order, the "first sheet" default of placement commands and
  every sheet list in the desktop. Hierarchical sheets and block symbols are out of scope (still in
  ERC `not_implemented`, `crates/eak-validation/src/erc.rs`).
  Locally labelled nets are named `<sheet name>/<label>` (ADR-0040). The PCB and the constraints
  refer to nets by name: footprint pad nets, tracks, vias, zones, net classes, and declared
  currents and voltages. Renaming a sheet therefore renames those nets.
- **Options considered:**
  1. A sheet order kept as `Schematic.sheet_order: Vec<Id>`. Rejected: it is a second structure
     that must agree with the `sheets` map, and undo would have to diff a whole section.
  2. **Chosen:** an `order: u32` on each `Sheet`. It is `#[serde(default)]` and omitted when 0.
     Sheets list by `(order, id)`, so every older file (all sheets at 0) keeps its id order with no
     schema bump, and the order is covered by the existing keyed-collection undo delta. No version
     bump keeps this slice clear of slice C4's schema v4. Files written with an `order` and read by
     an older build lose only the order (serde ignores unknown fields).
  3. Net names keyed by sheet id instead of sheet name. Rejected: it changes every existing local
     net name and the names users see.
  4. **Chosen for rename:** `RenameSheet` builds the netlist before and after the rename, matches
     each net by one of its pins (or else one of its labels), and rewrites every PCB and
     constraint reference of a renamed net in the same transaction. The board stays connected the
     same way. Undo restores names and references together.
  5. `DeleteSheet` with a `confirm` flag that deletes the contents itself. Rejected: it would be a
     second delete path for symbols, wires and labels. **Chosen:** `DeleteSheet` refuses while
     anything is on the sheet (the refusal lists what is there) and for the last sheet. The UI
     confirms, then submits one `Batch` that deletes the contents with the existing commands and
     then the sheet. That is one revision and one undo step.
- **Decision:**
  - `Sheet.order: u32`, default 0, not written when 0. `Project::sheets_in_order()` lists the
    sheets; `sheet_number` and `first_sheet_id` use it, as do the schematic PDF, the AI project
    summary and the desktop (`shell/sheets.ts` `sheetsInOrder`). `AddSheet` appends: its `order`
    is one past the largest.
  - `RenameSheet { sheet_id, name }` (EngineeringImpacting): trims the name. It refuses a blank,
    unchanged or duplicate name and an unknown sheet. Renamed nets are reported as a warning
    (`A → B`).
  - `DeleteSheet { sheet_id }` (Destructive): only for an empty sheet that is not the last one.
  - `ReorderSheets { sheet_ids }` (ReversibleLocal): refused unless every sheet id appears
    exactly once, or when nothing would change. It sets `order` to 1..n.
  - The tool schemas, harness phase lists and task-invalidation map name the three commands.
    Rename invalidates nets; delete and reorder invalidate the schematic.
  - Migration: none needed (`migrate.rs` unchanged). A v3 file without `order` loads with every
    sheet at 0 and keeps its id order. It re-saves byte-identically while nothing is reordered.
- **Consequences:** The Pages tab offers Add sheet, Rename, Move up / Move down and Delete. A
  non-empty delete asks first and lists what goes. The netlist's connectivity is unchanged by a
  rename or a reorder: a reorder changes no name, and a rename changes only the names of that
  sheet's local nets. Tests: `crates/eak-runtime/tests/sheets.rs`, the `eak-core`
  `sheets_list_by_order_then_id_and_order_0_is_not_written` test,
  `apps/desktop/src/shell/sheets.test.tsx` and the ui-evidence scenario
  `apps/desktop/e2e/scenarios/b6-sheets.json`.
- **Source requirement:** Product spec EDA workspace (multi-sheet schematics); Phase 1 plan B6.
