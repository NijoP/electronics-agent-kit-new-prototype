//! Deterministic reference design built only through the public command API.
//! Used by tests, the CLI (`eak demo`) and the desktop "Open sample project" action.
//!
//! Circuit: 5 V input header → AMS1117-3.3 LDO → ATtiny85 with decoupling, a reset
//! pull-up, an LED on PB0 and a 4-pin I/O header.

use crate::command::Command as C;
use crate::engine::{Actor, CommandRequest, CommandResponse, Engine};
use eak_core::{CopperLayer, Id, LabelKind, Point, Rotation};

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
pub fn build_pcb(engine: &mut Engine) -> Result<Vec<String>, String> {
    let mut b = Builder {
        engine,
        actor: Actor {
            kind: crate::engine::ActorKind::Cli,
            name: "fixture".into(),
        },
    };
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
    warnings.push(format!("revision {}", b.engine.project().revision));
    Ok(warnings)
}
