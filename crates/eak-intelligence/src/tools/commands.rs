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
use eak_runtime::{Command, OperationClass};
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
        }
    }
}

fn sheet() -> Value {
    nullable(string("sheet id; omit for the first sheet"))
}
fn id(desc: &str) -> Value {
    string(desc)
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
        AddSheet { .. } => (
            D::Schematic,
            "Add a schematic sheet.",
            obj(&[("name", string("sheet name"))], &["name"]),
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
            "Move a symbol. Attached wires are not dragged.",
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
                ],
                &["symbol_id", "pin", "name"],
            ),
        ),
        DeleteNetLabel { .. } => (
            D::Schematic,
            "Delete a net label.",
            obj(&[("label_id", id("label id"))], &["label_id"]),
        ),
        SetNoConnect { .. } => (
            D::Schematic,
            "Mark a pin as intentionally unconnected.",
            obj(
                &[("symbol_id", id("symbol id")), ("pin", string("pin number"))],
                &["symbol_id", "pin"],
            ),
        ),
        Annotate => (
            D::Schematic,
            "Assign reference designators to symbols whose designator ends in '?'.",
            obj(&[], &[]),
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
            "Add a copper track segment on a net.",
            obj(
                &[
                    ("layer", copper_layer()),
                    ("a", point("start")),
                    ("b", point("end")),
                    ("width", nullable(nm("track width; omit for the net class width"))),
                    ("net", string("net name")),
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
        SetManufacturing { .. } => (
            D::Manufacturing,
            "Set the manufacturing configuration.",
            obj(&[("config", manufacturing())], &["config"]),
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
        ImportLibraryComponent {
            component_id: "c".into(),
        },
        UpsertComponentDefinition {
            definition: Box::new(def),
        },
        AddSheet { name: "s".into() },
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
        },
        LabelPin {
            symbol_id: "s".into(),
            pin: "1".into(),
            name: "GND".into(),
            kind: LabelKind::Net,
        },
        DeleteNetLabel {
            label_id: "l".into(),
        },
        SetNoConnect {
            symbol_id: "s".into(),
            pin: "3".into(),
        },
        Annotate,
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
        AutoPlace {
            grid: Some(500_000),
        },
        AutoRoute {
            nets: Some(vec!["N".into()]),
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
        SetManufacturing {
            config: ManufacturingConfig::default(),
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
        // STATUS.md records 38 commands in the runtime.
        assert_eq!(tools.len(), 38);
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
        assert_eq!(c, Command::Annotate);
    }
}
