//! Typed tools generated from `eak_runtime::Command`: one tool per variant. The tool name
//! is the command's serde tag (`Command::name`), the risk class is `Command::class`, and the
//! input schema mirrors the variant's fields. Arguments are decoded with serde into the real
//! `Command` type, so the agent can only express what the command set can express
//! (Execution Instructions §7.2 "The AI must not write arbitrary design JSON").
//!
//! `describe` matches every variant without a wildcard: adding a command to the runtime
//! fails to compile here until its tool schema exists. Tests check every schema against
//! the serde shape of the real type.

use super::schema::*;
use crate::message::ToolSpec;
use eak_core::*;
use eak_runtime::{AnnotateOrder, AnnotateScope, Command, OperationClass};
use serde_json::{json, Value};

/// Part of the design a command touches; drives which verification should run afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Project,
    Library,
    Schematic,
    Pcb,
    Constraints,
    Manufacturing,
    /// A `Batch` may touch any part of the design.
    Mixed,
}

impl Domain {
    /// Checks the user should expect to (re)run after this kind of change (Spec §6.1
    /// "what validation will run").
    pub fn checks(self) -> &'static [&'static str] {
        match self {
            Domain::Project => &[],
            Domain::Library => &["ERC", "DRC"],
            Domain::Schematic => &["ERC"],
            Domain::Pcb => &["DRC", "MECH"],
            Domain::Constraints => &["DRC"],
            Domain::Manufacturing => &["DRC"],
            Domain::Mixed => &["ERC", "DRC", "MECH"],
        }
    }
}

fn sheet() -> Value {
    nullable(string("sheet id; omit for the first sheet"))
}
fn id(desc: &str) -> Value {
    string(desc)
}
fn object_class() -> Value {
    enumeration(
        &[
            "track", "smd_pad", "th_pad", "via", "zone", "hole", "outline", "silk",
        ],
        "clearance-matrix object class",
    )
}

/// One `Batch` step: `type` names any non-batch command tool and the remaining properties
/// are that tool's arguments (decoded and checked by serde, like a direct tool call).
fn batch_step() -> Value {
    let names: Vec<String> = command_samples()
        .iter()
        .filter(|c| !matches!(c, Command::Batch { .. }))
        .map(Command::name)
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    json!({
        "type": "object",
        "properties": {"type": enumeration(&names, "command tool name")},
        "required": ["type"],
        "additionalProperties": true,
        "description": "a command: its tool name in `type` plus that tool's arguments",
    })
}

/// Domain, description and input schema of a command variant. The values inside `c` are
/// ignored; only the variant matters.
pub fn describe(c: &Command) -> (Domain, &'static str, Value) {
    use Command::*;
    use Domain as D;
    match c {
        SetProjectName { .. } => (
            D::Project,
            "Rename the project.",
            obj(&[("name", string("new project name"))], &["name"]),
        ),
        AddRequirement { .. } => (
            D::Project,
            "Record a design requirement (free text) on the project.",
            obj(&[("text", string("requirement text"))], &["text"]),
        ),
        SetProjectProperty { .. } => (
            D::Project,
            "Set a free project property (company, drawn by, ...) that title blocks can show; a null or blank value removes it.",
            obj(
                &[("key", string("property key")), ("value", nullable(string("value; null removes")))],
                &["key"],
            ),
        ),
        SetRequirementField { .. } => (
            D::Project,
            "Record one structured requirement field (schema v9): `path` is a catalogue key such as supply.input_voltage, or `<item id>/<field>` for a rail or interface (see get_requirements for the catalogue and paths). The whole entry is replaced; quantities carry a unit, a value never stands in for a missing one, and EAK stamps the time and revision. An agent records UNKNOWN or NEEDS_CONFIRMATION (source kind agent_proposal or calculation for its own values); only a person records CONFIRMED or NOT_APPLICABLE; DERIVED is refused for everyone (only a deterministic evaluator writes it, and none exists yet). NEEDS_CONFIRMATION needs a source, the owner who must confirm it and risk_if_wrong; a recorded UNKNOWN needs its owner.",
            obj(
                &[
                    ("path", string("requirement path")),
                    ("status", req_status()),
                    ("value", nullable(req_value())),
                    ("source", nullable(req_source())),
                    ("derived_from", array(string("requirement path"), "inputs of a DERIVED value")),
                    ("owner", nullable(string("person responsible for the value: who must confirm it, or who confirmed it"))),
                    ("risk_if_wrong", nullable(string("what goes wrong if the value is wrong (NEEDS_CONFIRMATION only, and required there)"))),
                    ("note", nullable(string("rationale, open question, or why not applicable"))),
                ],
                &["path", "status"],
            ),
        ),
        ClearRequirementField { .. } => (
            D::Project,
            "Remove a recorded requirement field: it is UNKNOWN again. Refused while a DERIVED value names it.",
            obj(&[("path", string("requirement path"))], &["path"]),
        ),
        UpsertRequirementItem { .. } => (
            D::Project,
            "Add a supply rail or external interface to the requirement record (omit item_id; returns the new id), or rename / re-parent one. Only a rail has a parent: the rail it is generated from (the rail tree).",
            obj(
                &[
                    ("item_id", nullable(string("existing item id; omit to add"))),
                    ("kind", req_item_kind()),
                    ("name", string("rail or interface name, e.g. 3V3, USB-C")),
                    ("parent", nullable(string("rails only: id of the rail it is generated from"))),
                ],
                &["kind", "name"],
            ),
        ),
        DeleteRequirementItem { .. } => (
            D::Project,
            "Remove a rail or interface and its recorded fields. Refused while a rail is generated from it or a DERIVED value names one of its fields.",
            obj(&[("item_id", string("requirement item id"))], &["item_id"]),
        ),
        ImportLibraryComponent { .. } => (
            D::Library,
            "Copy a built-in library component into the project library.",
            obj(&[("component_id", id("built-in component id"))], &["component_id"]),
        ),
        UpsertComponentDefinition { .. } => (
            D::Library,
            "Add or replace a project component definition (symbol, footprint, pin map). Existing instances must keep their pins.",
            obj(&[("definition", component_def())], &["definition"]),
        ),
        SetComponentProperties { .. } => (
            D::Library,
            "Set manufacturer, MPN, Distributor number and datasheet URL of a project library component (null or blank clears a field). Provenance and ratings are unchanged.",
            obj(
                &[
                    ("component_id", id("project library component id")),
                    ("manufacturer", nullable(string("manufacturer"))),
                    ("mpn", nullable(string("manufacturer part number"))),
                    ("distributor", nullable(string("Distributor part number"))),
                    ("datasheet", nullable(string("datasheet URL"))),
                ],
                &["component_id", "manufacturer", "mpn", "distributor", "datasheet"],
            ),
        ),
        AssignComponentFootprint { .. } => (
            D::Library,
            "Assign a footprint and symbol-pin -> pad map to a project library component. Refused while PCB footprints of the component exist; the result must pass library validation.",
            obj(
                &[
                    ("component_id", id("project library component id")),
                    ("footprint", footprint_def()),
                    ("pin_map", pin_map()),
                ],
                &["component_id", "footprint", "pin_map"],
            ),
        ),
        SetPadExpansion { .. } => (
            D::Library,
            "Set one pad's solder-mask and paste (stencil) expansion in a project library footprint, per side in nm (negative shrinks the opening). Null clears it: the mask then uses the board-wide manufacturing mask expansion and the paste aperture equals the pad. Paste applies to SMD pads only; at most 1 mm either way. Allowed while the part is placed on the PCB.",
            obj(
                &[
                    ("component_id", id("project library component id")),
                    ("pad", string("pad number")),
                    ("mask_expansion", nullable(nm("solder-mask expansion per side; null = board default"))),
                    ("paste_expansion", nullable(nm("paste expansion per side; null = none (aperture = pad)"))),
                ],
                &["component_id", "pad"],
            ),
        ),
        AddSheet { .. } => (
            D::Schematic,
            "Add a schematic sheet.",
            obj(&[("name", string("sheet name"))], &["name"]),
        ),
        RenameSheet { .. } => (
            D::Schematic,
            "Rename a sheet (names are unique). Nets named only by the sheet's local labels (<sheet>/<label>) are renamed on the PCB and in the constraints too.",
            obj(
                &[("sheet_id", id("sheet id")), ("name", string("new sheet name"))],
                &["sheet_id", "name"],
            ),
        ),
        DeleteSheet { .. } => (
            D::Schematic,
            "Delete an empty sheet. Refused while anything is on it (delete its contents first, in the same Batch) and for the last sheet.",
            obj(&[("sheet_id", id("sheet id"))], &["sheet_id"]),
        ),
        ReorderSheets { .. } => (
            D::Schematic,
            "Put the sheets in a new order; lists every sheet id exactly once.",
            obj(
                &[("sheet_ids", array(id("sheet id"), "every sheet id, in the new order"))],
                &["sheet_ids"],
            ),
        ),
        PlaceSymbol { .. } => (
            D::Schematic,
            "Place a component symbol on a sheet (imports a built-in component if needed).",
            obj(
                &[
                    ("sheet_id", sheet()),
                    ("component_id", id("component id (project or built-in library)")),
                    ("position", point("symbol origin")),
                    ("rotation", rotation()),
                    ("refdes", nullable(string("reference designator; omit to auto-assign"))),
                    ("value", nullable(string("value; omit for the component default"))),
                ],
                &["component_id", "position"],
            ),
        ),
        MoveSymbol { .. } => (
            D::Schematic,
            "Move a symbol. Attached wires are not dragged: to drag them, send a Batch with MoveWire steps first and the MoveSymbol last.",
            obj(
                &[("symbol_id", id("symbol id")), ("position", point("new origin"))],
                &["symbol_id", "position"],
            ),
        ),
        RotateSymbol { .. } => (
            D::Schematic,
            "Set a symbol's rotation.",
            obj(
                &[("symbol_id", id("symbol id")), ("rotation", rotation())],
                &["symbol_id", "rotation"],
            ),
        ),
        FlipSymbol { .. } => (
            D::Schematic,
            "Mirror a symbol about a sheet axis through its origin (horizontal swaps left and right, vertical swaps top and bottom). Attached wires are not dragged: send a Batch with MoveWire steps first and the FlipSymbol last.",
            obj(
                &[
                    ("symbol_id", id("symbol id")),
                    ("axis", enumeration(&["horizontal", "vertical"], "flip axis")),
                ],
                &["symbol_id", "axis"],
            ),
        ),
        SetSymbolValue { .. } => (
            D::Schematic,
            "Set a symbol's value text (e.g. 10k).",
            obj(
                &[("symbol_id", id("symbol id")), ("value", string("value"))],
                &["symbol_id", "value"],
            ),
        ),
        SetRefdes { .. } => (
            D::Schematic,
            "Set a symbol's reference designator (must be unique).",
            obj(
                &[("symbol_id", id("symbol id")), ("refdes", string("reference designator"))],
                &["symbol_id", "refdes"],
            ),
        ),
        DeleteSymbol { .. } => (
            D::Schematic,
            "Delete a symbol and the footprint realising it. DESTRUCTIVE.",
            obj(&[("symbol_id", id("symbol id"))], &["symbol_id"]),
        ),
        AddWire { .. } => (
            D::Schematic,
            "Draw a straight wire segment between two points.",
            obj(
                &[("sheet_id", sheet()), ("a", point("start")), ("b", point("end"))],
                &["a", "b"],
            ),
        ),
        DeleteWire { .. } => (
            D::Schematic,
            "Delete a wire segment.",
            obj(&[("wire_id", id("wire id"))], &["wire_id"]),
        ),
        MoveWire { .. } => (
            D::Schematic,
            "Set both end points of a wire segment (move it or stretch one end).",
            obj(
                &[("wire_id", id("wire id")), ("a", point("new start")), ("b", point("new end"))],
                &["wire_id", "a", "b"],
            ),
        ),
        ConnectPins { .. } => (
            D::Schematic,
            "Connect two pins on the same sheet with an L-shaped wire that avoids other pins.",
            obj(
                &[
                    ("a_symbol", id("first symbol id")),
                    ("a_pin", string("first pin number")),
                    ("b_symbol", id("second symbol id")),
                    ("b_pin", string("second pin number")),
                ],
                &["a_symbol", "a_pin", "b_symbol", "b_pin"],
            ),
        ),
        AddNetLabel { .. } => (
            D::Schematic,
            "Place a net label at a point.",
            obj(
                &[
                    ("sheet_id", sheet()),
                    ("name", string("net name")),
                    ("position", point("label position")),
                    ("kind", label_kind()),
                    ("scope", net_scope()),
                    ("power_style", nullable(power_style())),
                ],
                &["name", "position"],
            ),
        ),
        LabelPin { .. } => (
            D::Schematic,
            "Place a net label directly on a pin (names the pin's net).",
            obj(
                &[
                    ("symbol_id", id("symbol id")),
                    ("pin", string("pin number")),
                    ("name", string("net name")),
                    ("kind", label_kind()),
                    ("scope", net_scope()),
                    ("power_style", nullable(power_style())),
                ],
                &["symbol_id", "pin", "name"],
            ),
        ),
        DeleteNetLabel { .. } => (
            D::Schematic,
            "Delete a net label.",
            obj(&[("label_id", id("label id"))], &["label_id"]),
        ),
        MoveNetLabel { .. } => (
            D::Schematic,
            "Move a net label (it names whatever wire or pin it then touches).",
            obj(
                &[("label_id", id("label id")), ("position", point("new position"))],
                &["label_id", "position"],
            ),
        ),
        RenameNetLabel { .. } => (
            D::Schematic,
            "Rename a net label in place (same id, position, kind and scope). PCB copper and constraints on a net the rename changes follow the new name; renaming to an existing net name joins the nets.",
            obj(
                &[("label_id", id("label id")), ("name", string("new net name"))],
                &["label_id", "name"],
            ),
        ),
        Group { .. } => (
            D::Schematic,
            "Group two or more symbols, wires and net labels of one sheet so they select and move together; an object is in at most one group. Grouping never changes connectivity.",
            obj(
                &[
                    ("members", array(id("symbol, wire or net label id"), "objects to group")),
                    ("name", nullable(string("group name; omit for Group N"))),
                ],
                &["members"],
            ),
        ),
        Ungroup { .. } => (
            D::Schematic,
            "Dissolve a group; its objects stay where they are.",
            obj(&[("group_id", id("group id"))], &["group_id"]),
        ),
        SetNoConnect { .. } => (
            D::Schematic,
            "Mark a pin as intentionally unconnected (refused if it is already marked).",
            obj(
                &[("symbol_id", id("symbol id")), ("pin", string("pin number"))],
                &["symbol_id", "pin"],
            ),
        ),
        DeleteNoConnect { .. } => (
            D::Schematic,
            "Remove a no-connect marker from a pin.",
            obj(&[("no_connect_id", id("no-connect id"))], &["no_connect_id"]),
        ),
        SetNetLabelScope { .. } => (
            D::Schematic,
            "Set how far a net label reaches: local (its sheet only), global or off_page (every sheet). Power ports always connect on every sheet.",
            obj(&[("label_id", id("label id")), ("scope", net_scope())], &["label_id", "scope"]),
        ),
        SetNetLabelKind { .. } => (
            D::Schematic,
            "Change a label between a net label and a power port in place (same id, name, position, scope, power style and group). Power ports connect on every sheet whatever their scope.",
            obj(&[("label_id", id("label id")), ("kind", label_kind())], &["label_id", "kind"]),
        ),
        SetPowerPortStyle { .. } => (
            D::Schematic,
            "Set (or clear with null) the drawing style of a power port label.",
            obj(
                &[("label_id", id("power port label id")), ("style", nullable(power_style()))],
                &["label_id"],
            ),
        ),
        SetFieldVisibility { .. } => (
            D::Schematic,
            "Show or hide one text field of a placed symbol (refdes and value are shown by default, the others hidden).",
            obj(
                &[("symbol_id", id("symbol id")), ("field", symbol_field()), ("visible", boolean("show the field"))],
                &["symbol_id", "field", "visible"],
            ),
        ),
        AddBus { .. } => (
            D::Schematic,
            "Draw a named bus (D[0..7] or {SDA,SCL}) as a polyline. A bus is not a conductor: members connect through labelled wires on bus entries; a global label with the bus name on the bus exports its members to every sheet.",
            obj(
                &[("sheet_id", sheet()), ("name", string("bus name")), ("points", points("2 or more vertices"))],
                &["name", "points"],
            ),
        ),
        MoveBus { .. } => (
            D::Schematic,
            "Replace a bus polyline (move or reshape it); entries left off the new path are reported by ERC-019.",
            obj(&[("bus_id", id("bus id")), ("points", points("new vertices"))], &["bus_id", "points"]),
        ),
        DeleteBus { .. } => (
            D::Schematic,
            "Delete a bus and its bus entries.",
            obj(&[("bus_id", id("bus id"))], &["bus_id"]),
        ),
        AddBusEntry { .. } => (
            D::Schematic,
            "Add a bus entry: bus_point must lie on the bus; a member wire attaches at wire_point and must carry a member name.",
            obj(
                &[("bus_id", id("bus id")), ("bus_point", point("point on the bus")), ("wire_point", point("wire attachment point"))],
                &["bus_id", "bus_point", "wire_point"],
            ),
        ),
        MoveBusEntry { .. } => (
            D::Schematic,
            "Move a bus entry (bus_point must lie on its bus).",
            obj(
                &[("entry_id", id("bus entry id")), ("bus_point", point("point on the bus")), ("wire_point", point("wire attachment point"))],
                &["entry_id", "bus_point", "wire_point"],
            ),
        ),
        DeleteBusEntry { .. } => (
            D::Schematic,
            "Delete a bus entry.",
            obj(&[("entry_id", id("bus entry id"))], &["entry_id"]),
        ),
        AddSheetGraphic { .. } => (
            D::Schematic,
            "Draw a line, shape or text on a sheet (drawing only; never part of connectivity).",
            obj(&[("sheet_id", sheet()), ("graphic", graphic())], &["graphic"]),
        ),
        MoveSheetGraphic { .. } => (
            D::Schematic,
            "Move a sheet drawing by an offset.",
            obj(&[("graphic_id", id("sheet graphic id")), ("by", point("offset"))], &["graphic_id", "by"]),
        ),
        DeleteSheetGraphic { .. } => (
            D::Schematic,
            "Delete a sheet drawing.",
            obj(&[("graphic_id", id("sheet graphic id"))], &["graphic_id"]),
        ),
        SetSheetFormat { .. } => (
            D::Schematic,
            "Set a sheet border, reference zones and title block (fields bound to project or sheet values, project properties or text); null removes the format.",
            obj(
                &[("sheet_id", id("sheet id")), ("format", nullable(drawing_format()))],
                &["sheet_id"],
            ),
        ),
        Annotate { .. } => (
            D::Schematic,
            "Assign reference designators. scope: unannotated (default: designators ending in '?'), all, or selection (the symbols listed). Numbering follows the sheet order, then order: across (default: by Y, then X) or down (by X, then Y). reset returns the designators in scope to '<prefix>?' instead. Designators outside the scope never change.",
            obj(
                &[
                    ("order", enumeration(&["across", "down"], "numbering order within a sheet")),
                    (
                        "scope",
                        enumeration(&["unannotated", "all", "selection"], "which symbols to number"),
                    ),
                    ("symbols", array(id("symbol id"), "symbols of the selection scope")),
                    ("reset", boolean("reset the designators in scope to '<prefix>?'")),
                ],
                &[],
            ),
        ),
        SetBoardOutline { .. } => (
            D::Pcb,
            "Set the board outline polygon (simple polygon, ≥ 3 vertices).",
            obj(&[("points", points("outline vertices"))], &["points"]),
        ),
        SetStackup { .. } => (
            D::Pcb,
            "Set the board stackup.",
            obj(&[("stackup", stackup())], &["stackup"]),
        ),
        SetLayerCount { .. } => (
            D::Pcb,
            "Rebuild the stackup as a default multilayer board (even count 2-16; inner layers 1 and n are planes). Refused while tracks, vias or zones use a layer that would disappear.",
            obj(
                &[
                    ("count", integer("number of copper layers")),
                    ("board_thickness", nullable(nm("finished thickness; omit to keep the current one"))),
                ],
                &["count"],
            ),
        ),
        SyncSchematicToPcb => (
            D::Pcb,
            "Forward-annotate: create/update/remove footprints and pad nets from the schematic.",
            obj(&[], &[]),
        ),
        SyncSchematicToPcbSelected { .. } => (
            D::Pcb,
            "Apply only the chosen changes of the schematic-to-PCB update (as an ECO preview lists them), as one revision. Refused if a change is no longer part of the update; a pad net of a new footprint needs that footprint chosen too.",
            obj(
                &[("changes", array(sync_change(), "changes to apply"))],
                &["changes"],
            ),
        ),
        MoveFootprint { .. } => (
            D::Pcb,
            "Move/rotate/flip a footprint. Tracks are not dragged; locked footprints refuse.",
            obj(
                &[
                    ("footprint_id", id("footprint id")),
                    ("position", point("new origin")),
                    ("rotation", nullable(rotation())),
                    ("side", nullable(side())),
                ],
                &["footprint_id", "position"],
            ),
        ),
        SetFootprintLocked { .. } => (
            D::Pcb,
            "Lock or unlock a footprint.",
            obj(
                &[("footprint_id", id("footprint id")), ("locked", boolean("locked"))],
                &["footprint_id", "locked"],
            ),
        ),
        AddTrack { .. } => (
            D::Pcb,
            "Add a copper track segment on a net (straight, or an arc when arc_deg is given).",
            obj(
                &[
                    ("layer", copper_layer()),
                    ("a", point("start")),
                    ("b", point("end")),
                    ("width", nullable(nm("track width; omit for the net class width"))),
                    ("net", string("net name")),
                    ("arc_deg", nullable(number("make it an arc of this sweep in degrees, positive counter-clockwise; omit for straight"))),
                ],
                &["layer", "a", "b", "net"],
            ),
        ),
        DeleteTrack { .. } => (
            D::Pcb,
            "Delete a track segment.",
            obj(&[("track_id", id("track id"))], &["track_id"]),
        ),
        AddVia { .. } => (
            D::Pcb,
            "Add a via on a net: through by default, or blind/buried by giving from and to layers.",
            obj(
                &[
                    ("position", point("via centre")),
                    ("net", string("net name")),
                    ("drill", nullable(nm("drill; omit for the net class value"))),
                    ("diameter", nullable(nm("pad diameter; omit for the net class value"))),
                    ("from", nullable(copper_layer())),
                    ("to", nullable(copper_layer())),
                ],
                &["position", "net"],
            ),
        ),
        DeleteVia { .. } => (
            D::Pcb,
            "Delete a via.",
            obj(&[("via_id", id("via id"))], &["via_id"]),
        ),
        AddZone { .. } => (
            D::Pcb,
            "Add a copper pour zone on a net.",
            obj(
                &[
                    ("layer", copper_layer()),
                    ("net", string("net name")),
                    ("outline", points("zone outline")),
                    ("clearance", nullable(nm("clearance to other nets; omit for the class value"))),
                ],
                &["layer", "net", "outline"],
            ),
        ),
        DeleteZone { .. } => (
            D::Pcb,
            "Delete a copper zone.",
            obj(&[("zone_id", id("zone id"))], &["zone_id"]),
        ),
        MoveTrack { .. } => (
            D::Pcb,
            "Drag a track segment by an offset; same-net track ends on its layer that met its old ends follow it.",
            obj(
                &[("track_id", id("track id")), ("by", point("offset to move by"))],
                &["track_id", "by"],
            ),
        ),
        EditTrack { .. } => (
            D::Pcb,
            "Change a track's width, layer, end points or arc sweep (arc_deg 0 = straight); omitted fields keep their value.",
            obj(
                &[
                    ("track_id", id("track id")),
                    ("width", nullable(nm("new width"))),
                    ("layer", nullable(copper_layer())),
                    ("a", nullable(point("new start"))),
                    ("b", nullable(point("new end"))),
                    ("arc_deg", nullable(number("arc sweep in degrees, positive counter-clockwise; 0 = straight"))),
                ],
                &["track_id"],
            ),
        ),
        MoveVia { .. } => (
            D::Pcb,
            "Move a via; same-net track ends that ended on it (on the layers it spans) follow it.",
            obj(
                &[("via_id", id("via id")), ("position", point("new via centre"))],
                &["via_id", "position"],
            ),
        ),
        EditZone { .. } => (
            D::Pcb,
            "Change a copper zone's outline, net, layer or clearance; omitted fields keep their value.",
            obj(
                &[
                    ("zone_id", id("zone id")),
                    ("outline", nullable(points("new zone outline"))),
                    ("net", nullable(string("new net name"))),
                    ("layer", nullable(copper_layer())),
                    ("clearance", nullable(nm("clearance to other nets"))),
                ],
                &["zone_id"],
            ),
        ),
        DeleteFootprint { .. } => (
            D::Pcb,
            "Remove a footprint from the board (refused while locked). Its tracks stay; while its symbol exists the next schematic-to-PCB sync places it again.",
            obj(&[("footprint_id", id("footprint id"))], &["footprint_id"]),
        ),
        AddRegion { .. } => (
            D::Pcb,
            "Add a keep-out and/or constraint region: forbid tracks, vias, components or zones inside it, and/or limit the width of tracks inside it.",
            obj(
                &[
                    ("name", nullable(string("region name"))),
                    ("outline", points("region outline (simple polygon)")),
                    ("layers", array(copper_layer(), "copper layers it applies to; empty or omitted = every layer")),
                    ("forbid", array(region_object(), "objects not allowed inside (keep-out)")),
                    ("min_track_width", nullable(nm("narrowest track allowed inside"))),
                    ("max_track_width", nullable(nm("widest track allowed inside"))),
                ],
                &["outline"],
            ),
        ),
        DeleteRegion { .. } => (
            D::Pcb,
            "Delete a keep-out or constraint region.",
            obj(&[("region_id", id("region id"))], &["region_id"]),
        ),
        AddPcbText { .. } => (
            D::Pcb,
            "Add text on the top silk, bottom silk or documentation layer (stroke font, anchored at the baseline centre).",
            obj(
                &[
                    ("layer", draw_layer()),
                    ("position", point("baseline centre")),
                    ("text", string("one line of text")),
                    ("size", nullable(nm("character height; omit for 1 mm"))),
                    ("stroke_width", nullable(nm("stroke width; omit for 0.16 mm"))),
                    ("rotation", nullable(rotation())),
                ],
                &["layer", "position", "text"],
            ),
        ),
        MovePcbText { .. } => (
            D::Pcb,
            "Move (and optionally rotate) PCB text.",
            obj(
                &[
                    ("text_id", id("PCB text id")),
                    ("position", point("new baseline centre")),
                    ("rotation", nullable(rotation())),
                ],
                &["text_id", "position"],
            ),
        ),
        DeletePcbText { .. } => (
            D::Pcb,
            "Delete PCB text.",
            obj(&[("text_id", id("PCB text id"))], &["text_id"]),
        ),
        AddDimension { .. } => (
            D::Pcb,
            "Add an aligned linear dimension between two points (default: documentation layer, mm, 2 decimals).",
            obj(
                &[
                    ("layer", nullable(draw_layer())),
                    ("a", point("first measured point")),
                    ("b", point("second measured point")),
                    ("offset", nullable(nm("distance of the dimension line from a-b, positive to the left of a->b"))),
                    ("unit", nullable(dimension_unit())),
                    ("precision", nullable(integer("decimals shown, 0-6"))),
                    ("text_size", nullable(nm("label height; omit for 1 mm"))),
                ],
                &["a", "b"],
            ),
        ),
        DeleteDimension { .. } => (
            D::Pcb,
            "Delete a dimension.",
            obj(&[("dimension_id", id("dimension id"))], &["dimension_id"]),
        ),
        AddBoardCutout { .. } => (
            D::Pcb,
            "Add a hole milled through the board (simple polygon inside the board outline, not touching it or another cut-out).",
            obj(&[("outline", points("cut-out outline"))], &["outline"]),
        ),
        DeleteBoardCutout { .. } => (
            D::Pcb,
            "Delete a board cut-out.",
            obj(&[("cutout_id", id("cut-out id"))], &["cutout_id"]),
        ),
        SetOutlineArc { .. } => (
            D::Pcb,
            "Make board outline edge `edge` (from vertex edge to the next vertex) an arc; null or 0 makes it straight.",
            obj(
                &[
                    ("edge", integer("edge index, 0-based")),
                    ("sweep_deg", nullable(number("arc sweep in degrees, positive counter-clockwise"))),
                ],
                &["edge"],
            ),
        ),
        AutoPlace { .. } => (
            D::Pcb,
            "Run EAK's constructive placer inside the board outline.",
            obj(&[("grid", nullable(nm("placement grid; default 0.5 mm")))], &[]),
        ),
        AutoRoute { .. } => (
            D::Pcb,
            "Run EAK's native two-layer router (optionally only some nets).",
            obj(
                &[("nets", nullable(array(string("net name"), "nets to route; omit for all")))],
                &[],
            ),
        ),
        StitchZones { .. } => (
            D::Pcb,
            "Stitch copper pours (DESK method): escape via + pad-to-via track for each SMD pad of the zone net that its own layer's fill does not reach, and a stitching via for each fill island with no via. Needs a zone of the net on two or more layers. Fill islands touching no copper of the net are dropped as dead copper.",
            obj(&[("net", nullable(string("zone net, e.g. GND; omit for every zoned net")))], &[]),
        ),
        ClearRouting { .. } => (
            D::Pcb,
            "Remove all tracks and vias, or those of one net. DESTRUCTIVE.",
            obj(&[("net", nullable(string("net name; omit for every net")))], &[]),
        ),
        SetDesignRules { .. } => (
            D::Constraints,
            "Change design rules; omitted fields are unchanged.",
            obj(
                &[
                    ("min_track_width", nullable(nm("minimum track width"))),
                    ("min_clearance", nullable(nm("minimum clearance"))),
                    ("min_drill", nullable(nm("minimum drill"))),
                    ("min_annular_ring", nullable(nm("minimum annular ring"))),
                    ("board_edge_clearance", nullable(nm("copper to board edge"))),
                    ("default_track_width", nullable(nm("default class track width"))),
                    ("default_clearance", nullable(nm("default class clearance"))),
                    ("temp_rise_c", nullable(number("allowed conductor temperature rise, °C"))),
                    ("max_component_height", nullable(nm("maximum component height"))),
                    ("clear_max_component_height", boolean("remove the height limit")),
                ],
                &[],
            ),
        ),
        UpsertNetClass { .. } => (
            D::Constraints,
            "Add or replace a net class (moves listed nets out of other classes).",
            obj(&[("class", net_class())], &["class"]),
        ),
        SetNetCurrent { .. } => (
            D::Constraints,
            "Declare (or clear with null) a net's continuous current; drives IPC-2221 width checks.",
            obj(
                &[("net", string("net name")), ("amps", nullable(number("current in A")))],
                &["net"],
            ),
        ),
        SetNetVoltage { .. } => (
            D::Constraints,
            "Declare (or clear with null) a net's peak working voltage; drives clearance checks.",
            obj(
                &[("net", string("net name")), ("volts", nullable(number("voltage in V")))],
                &["net"],
            ),
        ),
        SetRuleConfig { .. } => (
            D::Constraints,
            "Enable or disable one verification rule (ERC-001, DRC-002, ...) or override its report severity; config null clears it back to the default. A disabled rule is reported as DISABLED, never as passing, and the gates that use it are NOT_RUN.",
            obj(
                &[
                    ("rule_id", string("rule id, e.g. ERC-001")),
                    (
                        "config",
                        nullable(obj(
                            &[
                                ("enabled", boolean("false disables the rule")),
                                (
                                    "severity",
                                    nullable(enumeration(
                                        &["error", "warning", "info"],
                                        "report severity of the rule's findings; null keeps the default",
                                    )),
                                ),
                            ],
                            &[],
                        )),
                    ),
                ],
                &["rule_id"],
            ),
        ),
        SetClearanceCell { .. } => (
            D::Constraints,
            "Set (or clear with null) the minimum clearance between two object classes in the DRC clearance matrix.",
            obj(
                &[
                    ("a", object_class()),
                    ("b", object_class()),
                    ("clearance", nullable(nm("minimum clearance"))),
                ],
                &["a", "b"],
            ),
        ),
        SetNetPairClearance { .. } => (
            D::Constraints,
            "Set (or clear with null) the minimum copper clearance between two named nets.",
            obj(
                &[
                    ("net_a", string("net name")),
                    ("net_b", string("another net name")),
                    ("clearance", nullable(nm("minimum clearance"))),
                ],
                &["net_a", "net_b"],
            ),
        ),
        SetDiffPair { .. } => (
            D::Constraints,
            "Declare a differential pair (replaces any pair sharing either net); an omitted figure is undeclared and its checks are UNKNOWN.",
            obj(&[("rule", diff_pair_rule())], &["rule"]),
        ),
        DeleteDiffPair { .. } => (
            D::Constraints,
            "Remove the differential pair that contains a net.",
            obj(&[("net", string("either net of the pair"))], &["net"]),
        ),
        SetViaSpans { .. } => (
            D::Constraints,
            "Replace the blind/buried via spans the router may use besides through vias (empty = through vias only).",
            obj(&[("spans", array(via_span(), "allowed via spans"))], &[]),
        ),
        SetEngineeringInputs { .. } => (
            D::Constraints,
            "Replace the engineering-check inputs (ambient, via plating, allowed drops, part dissipation, insulation basis, enclosure). Send the whole set: an omitted field is cleared and its checks become UNKNOWN.",
            engineering_inputs(),
        ),
        SetManufacturing { .. } => (
            D::Manufacturing,
            "Set the manufacturing configuration.",
            obj(&[("config", manufacturing())], &["config"]),
        ),
        SetRoutingOptions { .. } => (
            D::Pcb,
            "Save the interactive route tool's options with the board: corners, obstacle mode, track width and via for new routes (null = net class) and loop removal. Changes no copper.",
            obj(&[("options", routing_options())], &["options"]),
        ),
        Batch { .. } => (
            D::Mixed,
            "Apply several EAK commands in order as ONE atomic edit: all succeed or nothing changes; one revision and one undo step. Each step is {\"type\": <command tool name>, ...that tool's arguments}. Batches do not nest; at most 1000 steps; the batch needs approval for its riskiest step.",
            obj(
                &[
                    ("commands", array(batch_step(), "steps, applied in order")),
                    ("label", nullable(string("short history label, e.g. 'Move 3 parts'"))),
                ],
                &["commands"],
            ),
        ),
    }
}

/// One representative value of every `Command` variant (the tool catalogue is built from
/// these; `describe` guarantees each variant has a schema).
pub fn command_samples() -> Vec<Command> {
    use Command::*;
    let p = Point::new(0, 0);
    let def = eak_core::builtin::components()
        .into_iter()
        .next()
        .expect("built-in library is not empty");
    vec![
        SetProjectName { name: "n".into() },
        AddRequirement { text: "t".into() },
        SetProjectProperty {
            key: "company".into(),
            value: Some("ACME".into()),
        },
        SetRequirementField {
            path: "supply.input_voltage".into(),
            status: ReqStatus::NeedsConfirmation,
            value: Some(ReqValue::Range {
                min: Some(4.5),
                nom: Some(5.0),
                max: Some(5.5),
                unit: ReqUnit::Volt,
            }),
            source: Some(ReqSource {
                kind: eak_core::requirements::SourceKind::Client,
                reference: "kick-off notes".into(),
            }),
            derived_from: vec![],
            owner: Some("Client PM".into()),
            risk_if_wrong: Some("the input protection and regulator are re-selected".into()),
            note: Some("USB bus power".into()),
        },
        ClearRequirementField {
            path: "supply.input_voltage".into(),
        },
        UpsertRequirementItem {
            item_id: None,
            kind: ReqItemKind::Rail,
            name: "3V3".into(),
            parent: Some("rail_vin".into()),
        },
        DeleteRequirementItem {
            item_id: "rail_3v3".into(),
        },
        ImportLibraryComponent {
            component_id: "c".into(),
        },
        SetComponentProperties {
            component_id: "c".into(),
            manufacturer: Some("m".into()),
            mpn: None,
            distributor: Some("C1".into()),
            datasheet: None,
        },
        AssignComponentFootprint {
            component_id: "c".into(),
            footprint: def.footprint.clone(),
            pin_map: def.pin_map.clone(),
        },
        SetPadExpansion {
            component_id: "c".into(),
            pad: "1".into(),
            mask_expansion: Some(-20_000),
            paste_expansion: None,
        },
        UpsertComponentDefinition {
            definition: Box::new(def),
        },
        AddSheet { name: "s".into() },
        RenameSheet {
            sheet_id: "sht".into(),
            name: "Power".into(),
        },
        DeleteSheet {
            sheet_id: "sht".into(),
        },
        ReorderSheets {
            sheet_ids: vec!["sht".into()],
        },
        PlaceSymbol {
            sheet_id: None,
            component_id: "c".into(),
            position: p,
            rotation: Rotation::R90,
            refdes: Some("R1".into()),
            value: None,
        },
        MoveSymbol {
            symbol_id: "s".into(),
            position: p,
        },
        RotateSymbol {
            symbol_id: "s".into(),
            rotation: Rotation::R180,
        },
        FlipSymbol {
            symbol_id: "s".into(),
            axis: FlipAxis::Vertical,
        },
        SetSymbolValue {
            symbol_id: "s".into(),
            value: "10k".into(),
        },
        SetRefdes {
            symbol_id: "s".into(),
            refdes: "R2".into(),
        },
        DeleteSymbol {
            symbol_id: "s".into(),
        },
        AddWire {
            sheet_id: Some("sht".into()),
            a: p,
            b: Point::new(1, 0),
        },
        DeleteWire {
            wire_id: "w".into(),
        },
        MoveWire {
            wire_id: "w".into(),
            a: p,
            b: Point::new(0, 1),
        },
        ConnectPins {
            a_symbol: "a".into(),
            a_pin: "1".into(),
            b_symbol: "b".into(),
            b_pin: "2".into(),
        },
        AddNetLabel {
            sheet_id: None,
            name: "VCC".into(),
            position: p,
            kind: LabelKind::Power,
            scope: NetScope::OffPage,
            power_style: Some(PowerStyle::Rail),
        },
        LabelPin {
            symbol_id: "s".into(),
            pin: "1".into(),
            name: "GND".into(),
            kind: LabelKind::Net,
            scope: NetScope::Local,
            power_style: None,
        },
        DeleteNetLabel {
            label_id: "l".into(),
        },
        MoveNetLabel {
            label_id: "l".into(),
            position: Point::new(2, 2),
        },
        RenameNetLabel {
            label_id: "l".into(),
            name: "VBAT".into(),
        },
        Group {
            members: vec!["s".into(), "w".into()],
            name: Some("Filter".into()),
        },
        Ungroup {
            group_id: "g".into(),
        },
        SetNoConnect {
            symbol_id: "s".into(),
            pin: "3".into(),
        },
        DeleteNoConnect {
            no_connect_id: "nc".into(),
        },
        SetNetLabelScope {
            label_id: "l".into(),
            scope: NetScope::Local,
        },
        SetNetLabelKind {
            label_id: "l".into(),
            kind: LabelKind::Power,
        },
        SetPowerPortStyle {
            label_id: "l".into(),
            style: Some(PowerStyle::Ground),
        },
        SetFieldVisibility {
            symbol_id: "s".into(),
            field: SymbolField::Mpn,
            visible: true,
        },
        AddBus {
            sheet_id: None,
            name: "D[0..7]".into(),
            points: vec![p, Point::new(10, 0)],
        },
        MoveBus {
            bus_id: "b".into(),
            points: vec![p, Point::new(0, 10), Point::new(10, 10)],
        },
        DeleteBus { bus_id: "b".into() },
        AddBusEntry {
            bus_id: "b".into(),
            bus_point: p,
            wire_point: Point::new(1, 1),
        },
        MoveBusEntry {
            entry_id: "be".into(),
            bus_point: Point::new(2, 0),
            wire_point: Point::new(3, 1),
        },
        DeleteBusEntry {
            entry_id: "be".into(),
        },
        AddSheetGraphic {
            sheet_id: Some("sht".into()),
            graphic: Graphic::Arc {
                center: p,
                radius: 5,
                start_deg: 0.0,
                end_deg: 90.0,
                stroke: Stroke {
                    width: 1,
                    style: LineStyle::Dashed,
                },
            },
        },
        MoveSheetGraphic {
            graphic_id: "g".into(),
            by: Point::new(1, 2),
        },
        DeleteSheetGraphic {
            graphic_id: "g".into(),
        },
        SetSheetFormat {
            sheet_id: "sht".into(),
            format: Some(DrawingFormat {
                title_block: vec![TitleBlockField {
                    label: "Company".into(),
                    value: FieldSource::Property {
                        key: "company".into(),
                    },
                }],
                ..DrawingFormat::standard()
            }),
        },
        Annotate {
            order: AnnotateOrder::Down,
            scope: AnnotateScope::Selection,
            symbols: vec!["s".into()],
            reset: false,
        },
        SetBoardOutline {
            points: vec![p, Point::new(1, 0), Point::new(1, 1)],
        },
        SetStackup {
            stackup: Stackup::default(),
        },
        SetLayerCount {
            count: 4,
            board_thickness: None,
        },
        SyncSchematicToPcb,
        SyncSchematicToPcbSelected {
            changes: vec![
                eak_runtime::command::SyncChange::AddFootprint {
                    symbol_id: "s".into(),
                },
                eak_runtime::command::SyncChange::PadNet {
                    symbol_id: "s".into(),
                    pad: "1".into(),
                },
            ],
        },
        MoveFootprint {
            footprint_id: "f".into(),
            position: p,
            rotation: Some(Rotation::R270),
            side: Some(Side::Bottom),
        },
        SetFootprintLocked {
            footprint_id: "f".into(),
            locked: true,
        },
        AddTrack {
            layer: CopperLayer::Bottom,
            a: p,
            b: Point::new(5, 5),
            width: Some(250_000),
            net: "N".into(),
            arc_deg: Some(-45.0),
        },
        DeleteTrack {
            track_id: "t".into(),
        },
        AddVia {
            position: p,
            net: "N".into(),
            drill: Some(300_000),
            diameter: None,
            from: Some(CopperLayer::Top),
            to: Some(CopperLayer::Inner(1)),
        },
        DeleteVia { via_id: "v".into() },
        AddZone {
            layer: CopperLayer::Top,
            net: "GND".into(),
            outline: vec![p, Point::new(1, 0), Point::new(1, 1)],
            clearance: Some(200_000),
        },
        DeleteZone {
            zone_id: "z".into(),
        },
        MoveTrack {
            track_id: "t".into(),
            by: Point::new(1_000_000, 0),
        },
        EditTrack {
            track_id: "t".into(),
            width: Some(300_000),
            layer: Some(CopperLayer::Inner(1)),
            a: Some(p),
            b: None,
            arc_deg: Some(90.0),
        },
        MoveVia {
            via_id: "v".into(),
            position: Point::new(2, 2),
        },
        EditZone {
            zone_id: "z".into(),
            outline: Some(vec![p, Point::new(2, 0), Point::new(2, 2)]),
            net: Some("GND".into()),
            layer: None,
            clearance: Some(250_000),
        },
        DeleteFootprint {
            footprint_id: "f".into(),
        },
        AddRegion {
            name: Some("antenna".into()),
            outline: vec![p, Point::new(1, 0), Point::new(1, 1)],
            layers: vec![CopperLayer::Top],
            forbid: vec![RegionObject::Tracks, RegionObject::Zones],
            min_track_width: None,
            max_track_width: Some(500_000),
        },
        DeleteRegion {
            region_id: "r".into(),
        },
        AddPcbText {
            layer: DrawLayer::TopSilk,
            position: p,
            text: "REV A".into(),
            size: Some(1_200_000),
            stroke_width: None,
            rotation: Some(Rotation::R90),
        },
        MovePcbText {
            text_id: "x".into(),
            position: Point::new(3, 3),
            rotation: None,
        },
        DeletePcbText {
            text_id: "x".into(),
        },
        AddDimension {
            layer: Some(DrawLayer::Documentation),
            a: p,
            b: Point::new(10, 0),
            offset: Some(2_000_000),
            unit: Some(DimensionUnit::Mil),
            precision: Some(1),
            text_size: None,
        },
        DeleteDimension {
            dimension_id: "d".into(),
        },
        AddBoardCutout {
            outline: vec![p, Point::new(1, 0), Point::new(1, 1)],
        },
        DeleteBoardCutout {
            cutout_id: "c".into(),
        },
        SetOutlineArc {
            edge: 1,
            sweep_deg: Some(45.0),
        },
        AutoPlace {
            grid: Some(500_000),
        },
        AutoRoute {
            nets: Some(vec!["N".into()]),
        },
        StitchZones {
            net: Some("GND".into()),
        },
        ClearRouting { net: None },
        SetDesignRules {
            min_track_width: Some(1),
            min_clearance: None,
            min_drill: Some(1),
            min_annular_ring: None,
            board_edge_clearance: Some(1),
            default_track_width: None,
            default_clearance: Some(1),
            temp_rise_c: Some(10.0),
            max_component_height: None,
            clear_max_component_height: false,
        },
        UpsertNetClass {
            class: Constraints::default().default_class,
        },
        SetNetCurrent {
            net: "N".into(),
            amps: Some(1.5),
        },
        SetNetVoltage {
            net: "N".into(),
            volts: None,
        },
        SetRuleConfig {
            rule_id: "ERC-001".into(),
            config: Some(RuleConfig {
                enabled: true,
                severity: Some(RuleSeverity::Warning),
            }),
        },
        SetClearanceCell {
            a: ObjectClass::Track,
            b: ObjectClass::SmdPad,
            clearance: Some(200_000),
        },
        SetNetPairClearance {
            net_a: "GND".into(),
            net_b: "VIN".into(),
            clearance: None,
        },
        SetDiffPair {
            rule: DiffPairRule {
                positive: "USB_DP".into(),
                negative: "USB_DN".into(),
                width: Some(200_000),
                gap: Some(150_000),
                target_zdiff_ohm: Some(90.0),
                zdiff_tolerance_pct: Some(10.0),
                max_skew: None,
            },
        },
        DeleteDiffPair {
            net: "USB_DN".into(),
        },
        SetViaSpans {
            spans: vec![ViaSpanRule {
                from: CopperLayer::Top,
                to: CopperLayer::Inner(1),
                drill: 150_000,
                diameter: 350_000,
            }],
        },
        SetEngineeringInputs {
            ambient_temp_c: Some(40.0),
            via_plating_um: Some(25.0),
            net_max_drop_v: [("VIN".to_string(), 0.1)].into_iter().collect(),
            part_dissipation_w: [("U1".to_string(), 0.5)].into_iter().collect(),
            insulation: Some(Insulation {
                standard: InsulationStandard::Iec60664_1,
                pollution_degree: 2,
                material_group: "IIIa".into(),
                altitude_m: 2000.0,
                coated: false,
            }),
            enclosure: Some(Enclosure {
                max_height_bottom: Some(2_000_000),
                mounting_holes: vec![MountingHoleSpec {
                    position: Point::mm(3.0, 3.0),
                    drill: 3_200_000,
                    tolerance: 100_000,
                    keepout_radius: 3_000_000,
                }],
            }),
        },
        SetManufacturing {
            config: ManufacturingConfig::default(),
        },
        SetRoutingOptions {
            options: RoutingOptions {
                bend: BendMode::Orthogonal90,
                conflict: ConflictMode::Push,
                width: Some(300_000),
                via: Some(ViaSize {
                    drill: 300_000,
                    diameter: 600_000,
                }),
                remove_loops: false,
            },
        },
        Batch {
            commands: vec![
                MoveSymbol {
                    symbol_id: "s".into(),
                    position: p,
                },
                Command::annotate(),
            ],
            label: Some("Move".into()),
        },
    ]
}

/// A command tool with its runtime metadata.
#[derive(Debug, Clone)]
pub struct CommandTool {
    pub spec: ToolSpec,
    pub class: OperationClass,
    pub domain: Domain,
}

fn class_note(class: OperationClass) -> &'static str {
    match class {
        OperationClass::ReadOnly => "read-only",
        OperationClass::ReversibleLocal => "reversible local edit; needs approval",
        OperationClass::EngineeringImpacting => {
            "engineering-impacting edit; needs approval and re-verification"
        }
        OperationClass::Destructive => "destructive edit; needs explicit approval",
    }
}

pub fn command_tools() -> Vec<CommandTool> {
    command_samples()
        .iter()
        .map(|c| {
            let (domain, description, input_schema) = describe(c);
            let class = c.class();
            CommandTool {
                spec: ToolSpec {
                    name: c.name(),
                    description: format!(
                        "{description} [EAK command; class: {}]",
                        class_note(class)
                    ),
                    input_schema,
                },
                class,
                domain,
            }
        })
        .collect()
}

/// Decode tool arguments into a typed `Command` via serde (the single source of truth for
/// the command shape). Errors are returned as text for the model to correct.
pub fn decode(name: &str, arguments: &Value) -> Result<Command, String> {
    let mut obj = match arguments {
        Value::Object(m) => m.clone(),
        Value::Null => Default::default(),
        other => {
            return Err(format!(
                "arguments for {name} must be a JSON object, got: {other}"
            ))
        }
    };
    obj.insert("type".into(), json!(name));
    serde_json::from_value::<Command>(Value::Object(obj))
        .map_err(|e| format!("invalid arguments for {name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::super::schema::check::{minimal, validate};
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn one_tool_per_command_variant_with_unique_names() {
        let tools = command_tools();
        let names: BTreeSet<_> = tools.iter().map(|t| t.spec.name.clone()).collect();
        assert_eq!(names.len(), tools.len(), "duplicate command sample");
        // 97 commands (incl. Batch, SetLayerCount, StitchZones, DeleteNoConnect, the 14 schema-v3
        // schematic commands of ADR-0040, the 16 PCB commands of ADR-0042 (15 schema-v4 edits plus
        // SyncSchematicToPcbSelected), the 3 sheet commands of ADR-0043, the 2 schema-v5 commands of
        // ADR-0044, the 3 group and rename commands of ADR-0045, the 3 schema-v7 rule commands
        // of ADR-0046, SetNetLabelKind of ADR-0049, the 4 schema-v8 commands of ADR-0055 and the 4
        // schema-v9 requirement commands of ADR-0057); keep in step with REQUIREMENTS_TRACEABILITY
        // INT-04.
        assert_eq!(tools.len(), 97);
        for t in &tools {
            assert!(t
                .spec
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'));
            assert_eq!(t.spec.input_schema["type"], "object");
        }
    }

    #[test]
    fn every_schema_accepts_the_serde_form_of_its_command() {
        for c in command_samples() {
            let (_, _, schema) = describe(&c);
            let mut v = serde_json::to_value(&c).unwrap();
            v.as_object_mut().unwrap().remove("type");
            validate(&schema, &v, &c.name()).unwrap();
            // And the args decode back into the identical command.
            assert_eq!(decode(&c.name(), &v).unwrap(), c);
        }
    }

    #[test]
    fn required_fields_are_sufficient_for_serde() {
        for c in command_samples() {
            let (_, _, schema) = describe(&c);
            let min = minimal(&schema);
            let decoded = decode(&c.name(), &min)
                .unwrap_or_else(|e| panic!("{}: minimal args {min} rejected: {e}", c.name()));
            assert_eq!(decoded.name(), c.name());
        }
    }

    #[test]
    fn decode_rejects_bad_arguments_with_a_message() {
        let e = decode("MoveSymbol", &json!({"symbol_id": "s"})).unwrap_err();
        assert!(e.contains("position"), "{e}");
        assert!(decode("MoveSymbol", &json!("text")).is_err());
        assert!(decode("NoSuchCommand", &json!({})).is_err());
        // A model cannot smuggle in a different variant through "type".
        let c = decode("Annotate", &json!({"type": "ClearRouting"})).unwrap();
        assert_eq!(c, Command::annotate());
    }

    #[test]
    fn batch_tool_decodes_typed_steps_and_lists_every_other_command() {
        let c = decode(
            "Batch",
            &json!({"commands": [
                {"type": "MoveSymbol", "symbol_id": "s", "position": {"x": 1, "y": 2}},
                {"type": "Annotate"}
            ]}),
        )
        .unwrap();
        assert_eq!(
            c,
            Command::Batch {
                commands: vec![
                    Command::MoveSymbol {
                        symbol_id: "s".into(),
                        position: Point::new(1, 2)
                    },
                    Command::annotate()
                ],
                label: None
            }
        );
        // A malformed step is reported, not dropped.
        let e = decode("Batch", &json!({"commands": [{"type": "MoveSymbol"}]})).unwrap_err();
        assert!(e.contains("Batch"), "{e}");
        // The step enum names every command tool except Batch itself.
        let step = batch_step();
        let names = step["properties"]["type"]["enum"].as_array().unwrap();
        assert_eq!(names.len(), command_tools().len() - 1);
        assert!(!names.contains(&json!("Batch")));
        assert_eq!(
            command_tools()
                .iter()
                .find(|t| t.spec.name == "Batch")
                .unwrap()
                .domain,
            Domain::Mixed
        );
    }
}
