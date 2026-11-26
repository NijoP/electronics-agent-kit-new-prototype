//! Deterministic reference design built only through the public command API.
//! Used by tests, the CLI (`eak demo`) and the desktop "Open sample project" action.
//!
//! Circuit: 5 V input header → AMS1117-3.3 LDO → ATtiny85 with decoupling, a reset
//! pull-up, an LED on PB0 and a 4-pin I/O header.

use crate::command::Command as C;
use crate::engine::{Actor, CommandRequest, CommandResponse, Engine};
use eak_core::{CopperLayer, Enclosure, Id, LabelKind, Point, Rating, Rotation};

pub struct Builder<'a> {
    pub engine: &'a mut Engine,
    pub actor: Actor,
}

impl Builder<'_> {
    pub fn run(&mut self, cmd: C) -> Result<CommandResponse, String> {
        let r = self.engine.execute(CommandRequest {
            expected_revision: self.engine.project().revision,
            actor: self.actor.clone(),
            command: cmd.clone(),
            approval: None,
        });
        if r.accepted {
            Ok(r)
        } else {
            Err(format!("{} rejected: {:?}", cmd.name(), r.error))
        }
    }
    fn place(&mut self, comp: &str, x: f64, y: f64, value: Option<&str>) -> Result<Id, String> {
        let r = self.run(C::PlaceSymbol {
            sheet_id: None,
            component_id: comp.into(),
            position: Point::mm(x, y),
            rotation: Rotation::R0,
            refdes: None,
            value: value.map(String::from),
        })?;
        Ok(r.created[0].clone())
    }
    fn label(&mut self, sym: &Id, pin: &str, name: &str, power: bool) -> Result<(), String> {
        self.run(C::LabelPin {
            symbol_id: sym.clone(),
            pin: pin.into(),
            name: name.into(),
            kind: if power {
                LabelKind::Power
            } else {
                LabelKind::Net
            },
            scope: Default::default(),
            power_style: None,
        })
        .map(|_| ())
    }
}

/// Build the schematic of the reference design.
pub fn build_schematic(engine: &mut Engine) -> Result<(), String> {
    let mut b = Builder {
        engine,
        actor: Actor {
            kind: crate::engine::ActorKind::Cli,
            name: "fixture".into(),
            session: None,
        },
    };
    b.run(C::SetProjectName {
        name: "EAK Reference — ATtiny85 LED board".into(),
    })?;
    b.run(C::AddRequirement {
        text: "Input 5 V DC via 2-pin header, ≤ 0.5 A".into(),
    })?;
    b.run(C::AddRequirement {
        text: "3.3 V logic rail from a linear regulator".into(),
    })?;

    let j1 = b.place("CONN_1x02_P2.54", 20.0, 60.0, None)?;
    let c1 = b.place("C_0805", 45.0, 50.0, Some("10u"))?;
    let u1 = b.place("AMS1117-3.3_SOT-223", 70.0, 60.0, None)?;
    let c2 = b.place("C_0805", 95.0, 50.0, Some("10u"))?;
    let u2 = b.place("ATTINY85_SOIC-8", 140.0, 60.0, None)?;
    let c3 = b.place("C_0603", 120.0, 85.0, Some("100n"))?;
    let r2 = b.place("R_0603", 110.0, 30.0, Some("10k"))?;
    let r1 = b.place("R_0603", 175.0, 40.0, Some("330"))?;
    let d1 = b.place("LED_0603", 200.0, 50.0, Some("Green"))?;
    let j2 = b.place("CONN_1x04_P2.54", 190.0, 75.0, None)?;

    // Power input.
    b.label(&j1, "1", "+5V", true)?;
    b.label(&j1, "2", "GND", true)?;
    b.label(&c1, "1", "+5V", false)?;
    b.label(&c1, "2", "GND", false)?;
    b.label(&u1, "3", "+5V", false)?;
    b.label(&u1, "1", "GND", false)?;
    b.label(&u1, "2", "+3V3", false)?;
    b.label(&c2, "1", "+3V3", false)?;
    b.label(&c2, "2", "GND", false)?;
    // MCU.
    b.label(&u2, "8", "+3V3", false)?;
    b.label(&u2, "4", "GND", false)?;
    b.label(&c3, "1", "+3V3", false)?;
    b.label(&c3, "2", "GND", false)?;
    b.label(&r2, "1", "+3V3", false)?;
    b.label(&r2, "2", "RESET", false)?;
    b.label(&u2, "1", "RESET", false)?;
    b.run(C::SetNoConnect {
        symbol_id: u2.clone(),
        pin: "2".into(),
    })?;
    b.run(C::SetNoConnect {
        symbol_id: u2.clone(),
        pin: "3".into(),
    })?;
    // LED: PB0 → R1 → LED anode; cathode → GND. R1–D1 joined by a real wire.
    b.label(&u2, "5", "LED_DRV", false)?;
    b.label(&r1, "1", "LED_DRV", false)?;
    // LED_0603 pin 2 = anode, pin 1 = cathode.
    b.run(C::ConnectPins {
        a_symbol: r1.clone(),
        a_pin: "2".into(),
        b_symbol: d1.clone(),
        b_pin: "2".into(),
    })?;
    b.label(&d1, "1", "GND", false)?;
    // I/O header.
    b.label(&j2, "1", "+3V3", false)?;
    b.label(&j2, "2", "GND", false)?;
    b.label(&j2, "3", "IO1", false)?;
    b.label(&j2, "4", "IO2", false)?;
    b.label(&u2, "6", "IO1", false)?;
    b.label(&u2, "7", "IO2", false)?;
    Ok(())
}

/// Board outline, constraints, placement, routing and ground pour.
/// Build the 2-layer reference board.
pub fn build_pcb(engine: &mut Engine) -> Result<Vec<String>, String> {
    build_pcb_layers(engine, 2)
}

/// Build the reference board on `copper_layers` layers (even, 2..=16). With 4 or more layers the
/// first inner layer is a GND plane and the last a +5V plane (both with thermal reliefs on their
/// pads), and one blind (top to In1) and one buried (In1 to the last inner layer) GND via are added.
/// Everything goes through typed commands.
pub fn build_pcb_layers(engine: &mut Engine, copper_layers: u8) -> Result<Vec<String>, String> {
    let mut b = Builder {
        engine,
        actor: Actor {
            kind: crate::engine::ActorKind::Cli,
            name: "fixture".into(),
            session: None,
        },
    };
    if copper_layers != 2 {
        b.run(C::SetLayerCount {
            count: copper_layers,
            board_thickness: Some(1_600_000),
        })?;
    }
    b.run(C::SetBoardOutline {
        points: vec![
            Point::mm(0.0, 0.0),
            Point::mm(40.0, 0.0),
            Point::mm(40.0, 30.0),
            Point::mm(0.0, 30.0),
        ],
    })?;
    b.run(C::SetNetCurrent {
        net: "+5V".into(),
        amps: Some(0.5),
    })?;
    b.run(C::SetNetCurrent {
        net: "+3V3".into(),
        amps: Some(0.3),
    })?;
    b.run(C::SetNetVoltage {
        net: "+5V".into(),
        volts: Some(5.0),
    })?;
    b.run(C::SetNetVoltage {
        net: "+3V3".into(),
        volts: Some(3.3),
    })?;
    b.run(C::SyncSchematicToPcb)?;
    b.run(C::AutoPlace {
        grid: Some(1_000_000),
    })?;
    let mut warnings = b.run(C::AutoRoute { nets: None })?.warnings;
    b.run(C::AddZone {
        layer: CopperLayer::Bottom,
        net: "GND".into(),
        outline: vec![
            Point::mm(0.5, 0.5),
            Point::mm(39.5, 0.5),
            Point::mm(39.5, 29.5),
            Point::mm(0.5, 29.5),
        ],
        clearance: Some(300_000),
    })?;
    if copper_layers >= 4 {
        let last_inner = CopperLayer::Inner(copper_layers - 2);
        let board = vec![
            Point::mm(0.5, 0.5),
            Point::mm(39.5, 0.5),
            Point::mm(39.5, 29.5),
            Point::mm(0.5, 29.5),
        ];
        for (layer, net) in [(CopperLayer::Inner(1), "GND"), (last_inner, "+5V")] {
            b.run(C::AddZone {
                layer,
                net: net.into(),
                outline: board.clone(),
                clearance: Some(300_000),
            })?;
        }
        b.run(C::AddVia {
            position: Point::mm(2.0, 28.0),
            net: "GND".into(),
            drill: None,
            diameter: None,
            from: Some(CopperLayer::Top),
            to: Some(CopperLayer::Inner(1)),
        })?;
        b.run(C::AddVia {
            position: Point::mm(4.0, 28.0),
            net: "GND".into(),
            drill: None,
            diameter: None,
            from: Some(CopperLayer::Inner(1)),
            to: Some(last_inner),
        })?;
    }
    warnings.push(format!("revision {}", b.engine.project().revision));
    Ok(warnings)
}

/// Declares, through typed commands, the inputs the checks need that the reference design leaves
/// open, so no rule is UNKNOWN for want of one (ADR-0048: a rule left UNKNOWN blocks the
/// release). Fixture values for this logic-level reference design, not engineering data:
/// - a voltage and a current for every copper net without one: GND 0 V and 0.3 A (the return of
///   the 3V3 rail), any other net 3.3 V and 0.01 A;
/// - the voltage rating its generic resistor, LED and headers lack (ERC-022), unverified;
/// - an enclosure with no bottom-side limit and no mounting holes (MECH-004).
///
/// Declared values are kept.
pub fn declare_check_inputs(engine: &mut Engine) -> Result<(), String> {
    let p = engine.project();
    let nets: std::collections::BTreeSet<String> = eak_engineering::copper::items(p)
        .into_iter()
        .map(|i| i.net)
        .filter(|n| !n.is_empty())
        .collect();
    let c = &p.constraints;
    let no_voltage: Vec<String> = nets
        .iter()
        .filter(|n| !c.net_voltages_v.contains_key(*n))
        .cloned()
        .collect();
    let no_current: Vec<String> = nets
        .iter()
        .filter(|n| !c.net_currents_a.contains_key(*n))
        .cloned()
        .collect();
    let mut b = Builder {
        engine,
        actor: Actor {
            kind: crate::engine::ActorKind::Cli,
            name: "fixture".into(),
            session: None,
        },
    };
    for net in no_voltage {
        let volts = if net == "GND" { 0.0 } else { 3.3 };
        b.run(C::SetNetVoltage {
            net,
            volts: Some(volts),
        })?;
    }
    for net in no_current {
        let amps = if net == "GND" { 0.3 } else { 0.01 };
        b.run(C::SetNetCurrent {
            net,
            amps: Some(amps),
        })?;
    }
    for (id, name, volts) in [
        ("R_0603", "voltage", 50.0),
        ("LED_0603", "reverse_voltage_max", 5.0),
        ("CONN_1x02_P2.54", "voltage", 50.0),
        ("CONN_1x04_P2.54", "voltage", 50.0),
    ] {
        let Some(def) = b.engine.project().library.get(id) else {
            continue;
        };
        if def.ratings.iter().any(|r| r.name == name) {
            continue;
        }
        let mut def = def.clone();
        def.ratings.push(Rating {
            name: name.into(),
            value: volts,
            unit: "V".into(),
            source: "EAK reference fixture, not engineering data".into(),
            verified: false,
        });
        b.run(C::UpsertComponentDefinition {
            definition: Box::new(def),
        })?;
    }
    let c = &b.engine.project().constraints;
    if c.enclosure.is_none() {
        let inputs = C::SetEngineeringInputs {
            ambient_temp_c: c.ambient_temp_c,
            via_plating_um: c.via_plating_um,
            net_max_drop_v: c.net_max_drop_v.clone(),
            part_dissipation_w: c.part_dissipation_w.clone(),
            insulation: c.insulation.clone(),
            enclosure: Some(Enclosure::default()),
        };
        b.run(inputs)?;
    }
    Ok(())
}
