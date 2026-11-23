# ADR-0040: EAK-IR schema v3, schematic primitives (symbol graphics, drawing format, buses, port scope)
- **Date:** 2026-11-23
- **Context:** Phase 1 slice B1 (plan gaps #8-#11). Schema v2 could only draw a symbol as one box
  (`body_half_w`/`body_half_h`), had no sheet border or title block, no sheet drawings or text, no
  bus, and every net label joined every same-name label on every sheet. The schematic UI slices
  B2-B6 and the Phase 2/3 work need these objects in the canonical IR first. This changes shared
  contracts (IR types, `Command`, the ERC report), so it needs an ADR and a migration
  (AGENTS.md). Semantics follow the clean-room EasyEDA behaviour notes (net-label scope §2.3,
  bus and bus entry §2.4), adapted to EAK.
- **Options considered:**
  1. A separate `NetPort` collection next to `labels`. Rejected: two collections that both name
     nets would be two representations of one thing, and every connectivity, ERC and UI path would
     have to read both.
  2. Replace `body_half_w`/`body_half_h` with graphics only. Rejected for this slice: every
     renderer, hit test and the selection boundary read the box extents, and changing them all
     here would mix the contract change with UI work (B2-B6). The extents stay as the body's
     bounding half-extents.
  3. Add the new objects to the existing IR with defaults that reproduce v2 exactly, and one
     migration step. **Chosen.**
- **Decision (IR, `eak-core`):**
  - `Graphic` (`graphic.rs`), tagged by `kind`: `line`, `polyline`, `arc`, `circle`,
    `rectangle`, `polygon`, `text`. Lines carry a `Stroke` (`width`, 0 = renderer default, and a
    `style`: solid / dashed / dotted / dash_dot); closed shapes carry `fill`. `Graphic::validate`
    refuses empty or degenerate shapes (fewer than 2 polyline or 3 polygon points, zero-length
    line, non-positive radius or text size, empty text, non-finite arc angles, inverted
    rectangle). One type serves symbol bodies and sheet drawings.
  - `SymbolDef.graphics: Vec<Graphic>` (symbol-local coordinates). `body_half_w`/`body_half_h`
    stay and mean the body's bounding half-extents.
  - `SymbolPin` gains `length` (nm), `orientation` (`right`/`left`/`up`/`down`: the direction from
    the connection point towards the body), `name_visible` and `number_visible` (default true).
    The electrical type was already there.
  - `SymbolDef::upgrade_box()` turns a v2 box symbol (no graphics, positive extents) into a
    rectangle graphic of exactly that box, and gives each pin outside the box the orientation and
    length that reach the box edge. Pin positions do not move, so connectivity is unchanged.
  - `Sheet.format: Option<DrawingFormat>`: `border`, `zone_columns` (numbered) and `zone_rows`
    (lettered), and `title_block: Vec<TitleBlockField { label, value: FieldSource }>`. A field is
    bound to the project name, revision, sheet name, sheet number, sheet count, a project property
    (`property { key }`) or fixed `text`. `Project::title_block_value` resolves a binding.
    `None` (every v2 sheet) draws no border.
  - `Project.properties: BTreeMap<String, String>` (title-block values such as company or
    drawn-by).
  - `Schematic.graphics: BTreeMap<Id, SheetGraphic { id, sheet_id, graphic }>` for sheet lines,
    shapes and text. They are drawings only and never take part in connectivity.
  - `Schematic.buses: BTreeMap<Id, Bus { id, sheet_id, name, points }>` (a polyline) and
    `Schematic.bus_entries: BTreeMap<Id, BusEntry { id, sheet_id, bus_id, bus_point,
    wire_point }>`. `bus_members(name)` reads `D[0..7]` (either direction, at most 1024 members)
    and `{SDA,SCL}` / `{SDA SCL}` lists.
  - `NetLabel` is the net port (`pub type NetPort = NetLabel`). It gains `scope`
    (`local` / `global` / `off_page`, default `global`, omitted from the file when global) and
    `power_style` (`ground` / `rail` / `chassis`, power ports only, omitted when unset).
  - `SymbolInstance.field_visibility: BTreeMap<SymbolField, bool>` for `refdes`, `value`, `name`,
    `footprint`, `manufacturer`, `mpn`. Only values that differ from the default are stored
    (refdes and value are shown by default, the others hidden); empty is omitted from the file.
- **Decision (connectivity, `eak-engineering::netlist`):**
  - Labels of one name on one sheet join whatever their scope. Across sheets, only labels that
    connect globally join: `global`, `off_page` and every power port (power flags are global, as
    in EasyEDA). A `local` label joins only its own sheet. This is the v2 behaviour for every v2
    label, because v2 labels load as `global`.
  - A net whose names all come from local labels is named `<sheet name>/<label>`, so two sheets
    can each have a local `EN` without a clash. Every net name is unique: a clash gets a `_2`, `_3`
    suffix instead of silently replacing another net (which v2 could do for a label named like
    an automatic name).
  - A bus is not a conductor: pins and wire ends touching a bus do not join it. A bus entry's
    `wire_point` is a connection node for wires (it counts as a touch, so the wire end is not
    dangling). A label lying on a bus is a bus port, not an electrical node. A bus with a
    non-local port on it exports its members: each member name `M` that names a net on that
    sheet joins the global `M`. `Netlist` reports `buses` (name, parsed members, exported, port
    labels) and `bus_entries` (entry -> the net its wire side reaches).
- **Decision (ERC, `eak-validation`):** new rules ERC-017 illegal bus name (error), ERC-018 bus
  entry wire unnamed or not a member of its bus (error), ERC-019 bus entry not on its bus
  (error), ERC-020 bus port name differs from the bus name (warning), ERC-021 off-page connector
  with no partner on another sheet (warning). Bus rules not yet checked are listed in
  `not_implemented`: members named on the sheet without a bus entry, graphical bus/wire overlap,
  bus objects off grid, bus net classes, bus connection through hierarchy.
- **Decision (`Command`, `eak-runtime`):** 14 new variants, 61 commands in total.
  `AddBus`/`MoveBus`/`DeleteBus` (deleting a bus deletes its entries, with a warning),
  `AddBusEntry`/`MoveBusEntry`/`DeleteBusEntry` (the bus point must lie on the bus),
  `AddSheetGraphic`/`MoveSheetGraphic` (translate by `by`)/`DeleteSheetGraphic`,
  `SetSheetFormat` (`null` removes the border), `SetProjectProperty` (`null` value removes),
  `SetFieldVisibility`, `SetNetLabelScope`, `SetPowerPortStyle`. `AddNetLabel` and `LabelPin`
  take optional `scope` and `power_style`. `UpsertComponentDefinition` runs `upgrade_box` on a
  definition without graphics and validates its graphics. Classes: drawings, format, properties,
  field visibility and power-port style are `ReversibleLocal`; bus, bus entry and scope changes
  are `EngineeringImpacting` (they change connectivity or ERC). The agent tools, the harness
  schematic-command list and the task-invalidation map (`Change::Net` for connectivity commands)
  include them. Undo/redo deltas cover the new collections and the `properties` section.
- **Migration (`migrate.rs`, v2 → v3):** every project library symbol goes through
  `upgrade_box`, and the new collections (`buses`, `bus_entries`, `graphics`) are added empty.
  Nothing else changes: labels load as global, sheets without a format, instances with default
  field visibility. Tests: a v2 project with wires, labels, power ports and no-connects migrates
  to v3 with the identical netlist and pad nets, and the migrated file is byte-stable on re-save.
- **Consequences:** v3 files are refused by older builds (as for v2). Built-in and KiCad-imported
  symbols carry their rectangle graphic. Renderers still draw the box from the extents; drawing
  `graphics`, the drawing format, buses and ports is UI work in slices B2-B6. Until then the new
  objects are reachable through commands (CLI, MCP, agent), not through the desktop UI, and no
  desktop control claims them.
- **Source requirement:** product spec, schematic capture (symbols, sheets, buses, net scope);
  Phase 1 plan slice B1 (gaps #8-#11).
