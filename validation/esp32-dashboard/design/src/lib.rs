//! EAK design driver for the ESP32-C3 environment dashboard board.
//!
//! Every design change is a typed `eak_runtime::Command` executed by `Engine::execute`;
//! nothing here edits EAK-IR or project files directly. The pin assignments come from the
//! firmware's single pin map (`dashboard_core::pins`).

pub mod parts;
pub mod report;

use dashboard_core::pins;
use eak_core::{CopperLayer, Id, LabelKind, NetClass, Point, Rotation, Side, Stackup};
use eak_runtime::{Actor, ActorKind, Command as C, CommandRequest, CommandResponse, Engine};
use std::collections::BTreeMap;

/// Board outline (mm): 52 x 44, origin bottom-left.
pub const BOARD_W: f64 = 52.0;
pub const BOARD_H: f64 = 44.0;

/// Log of every command the driver executed (name, revision after, warnings).
#[derive(Default, Debug)]
pub struct BuildLog {
    pub commands: Vec<(String, u64, Vec<String>)>,
}

pub struct Driver<'a> {
    pub engine: &'a mut Engine,
    pub actor: Actor,
    pub log: BuildLog,
    /// refdes -> symbol id
    pub symbols: BTreeMap<String, Id>,
}

impl<'a> Driver<'a> {
    pub fn new(engine: &'a mut Engine) -> Self {
        Driver {
            engine,
            actor: Actor {
                kind: ActorKind::Cli,
                name: "esp32-dashboard-design".into(),
            },
            log: BuildLog::default(),
            symbols: BTreeMap::new(),
        }
    }

    pub fn run(&mut self, cmd: C) -> Result<CommandResponse, String> {
        let r = self.engine.execute(CommandRequest {
            expected_revision: self.engine.project().revision,
            actor: self.actor.clone(),
            command: cmd.clone(),
            approval: None,
        });
        if !r.accepted {
            // The engine refuses no-op commands; declaring a value that is already the
            // default (e.g. the stackup) is not an error for this driver.
            if let Some(eak_runtime::CommandError::NothingTo(why)) = &r.error {
                self.log.commands.push((cmd.name(), r.revision_after, vec![format!("no-op: {why}")]));
                return Ok(r);
            }
            return Err(format!("{} rejected: {:?}", cmd.name(), r.error));
        }
        self.log
            .commands
            .push((cmd.name(), r.revision_after, r.warnings.clone()));
        Ok(r)
    }

    fn place(&mut self, refdes: &str, comp: &str, value: Option<&str>, x: f64, y: f64) -> Result<Id, String> {
        let r = self.run(C::PlaceSymbol {
            sheet_id: None,
            component_id: comp.into(),
            position: Point::mm(x, y),
            rotation: Rotation::R0,
            refdes: Some(refdes.into()),
            value: value.map(String::from),
        })?;
        let id = r.created[0].clone();
        self.symbols.insert(refdes.into(), id.clone());
        Ok(id)
    }

    fn net(&mut self, refdes: &str, pin: &str, net: &str) -> Result<(), String> {
        self.label(refdes, pin, net, LabelKind::Net)
    }

    fn label(&mut self, refdes: &str, pin: &str, net: &str, kind: LabelKind) -> Result<(), String> {
        let id = self.symbols[refdes].clone();
        self.run(C::LabelPin {
            symbol_id: id,
            pin: pin.into(),
            name: net.into(),
            kind,
        })
        .map(|_| ())
    }

    fn nc(&mut self, refdes: &str, pin: &str) -> Result<(), String> {
        let id = self.symbols[refdes].clone();
        self.run(C::SetNoConnect {
            symbol_id: id,
            pin: pin.into(),
        })
        .map(|_| ())
    }

    fn two(&mut self, refdes: &str, a: &str, b: &str) -> Result<(), String> {
        self.net(refdes, "1", a)?;
        self.net(refdes, "2", b)
    }

    fn footprint_of(&self, refdes: &str) -> Id {
        let sym = &self.symbols[refdes];
        self.engine
            .project()
            .pcb
            .footprints
            .values()
            .find(|f| &f.symbol_id == sym)
            .map(|f| f.id.clone())
            .unwrap_or_else(|| panic!("no footprint for {refdes}"))
    }

    fn put(&mut self, refdes: &str, x: f64, y: f64, rot: Rotation) -> Result<(), String> {
        let fp = self.footprint_of(refdes);
        self.run(C::MoveFootprint {
            footprint_id: fp.clone(),
            position: Point::mm(x, y),
            rotation: Some(rot),
            side: Some(Side::Top),
        })?;
        self.run(C::SetFootprintLocked {
            footprint_id: fp,
            locked: true,
        })
        .map(|_| ())
    }
}

pub const REQUIREMENTS: &[&str] = &[
    "R1 Power: single USB Type-C 5 V input (USB 2.0 default power, <= 500 mA), 5.1 k CC pull-downs (UFP/sink).",
    "R2 Logic rail: 3.3 V +-5 % for the ESP32-C3 module (datasheet range 3.0-3.6 V) able to supply >= 0.5 A (datasheet Table 6-2).",
    "R3 MCU: ESP32-C3 (RISC-V) module with on-chip USB Serial/JTAG used for programming and the telemetry link (no USB-UART bridge).",
    "R4 Sensing: SHT40 temperature/humidity sensor (I2C 0x44) on an external 4-pin header, away from the module/LDO heat; VBUS voltage monitor on ADC1.",
    "R5 Local UI: status LED, BOOT/user button (GPIO9), reset button (EN) and a 4-pin header for a 0.96 in SSD1306 I2C OLED.",
    "R6 Dashboard: firmware streams CRC-protected telemetry frames at 1 Hz; a host dashboard shows live values, history, statistics, link health and alarms.",
    "R7 Mechanical: 52 x 44 mm 2-layer FR-4 1.6 mm board, 4 x M3 mounting holes, module antenna overhanging the board edge, max component height 12 mm.",
    "R8 Fabrication: generic low-cost 2-layer rules (0.15 mm track/space, 0.3 mm drill), GND pours on both layers.",
];

/// Schematic: symbols on a 45 x 32 mm grid; connectivity by pin labels.
pub fn build_schematic(d: &mut Driver) -> Result<(), String> {
    for r in REQUIREMENTS {
        d.run(C::AddRequirement { text: (*r).into() })?;
    }
    for def in parts::all() {
        d.run(C::UpsertComponentDefinition {
            definition: Box::new(def),
        })?;
    }

    let cells: &[(&str, &str, Option<&str>)] = &[
        ("U1", parts::ESP32_ID, None),
        ("J1", parts::USBC_ID, None),
        ("U2", "AMS1117-3.3_SOT-223", None),
        ("C1", "C_0805", Some("10u")),
        ("C2", "C_0805", Some("22u")),
        ("C3", "C_0805", Some("10u")),
        ("C4", "C_0603", Some("100n")),
        ("C5", "C_0603", Some("1u")),
        ("C6", "C_0603", Some("100n")),
        ("R1", "R_0603", Some("5.1k")),
        ("R2", "R_0603", Some("5.1k")),
        ("R3", "R_0603", Some("10k")),
        ("R4", "R_0603", Some("10k")),
        ("R5", "R_0603", Some("10k")),
        ("R6", "R_0603", Some("10k")),
        ("R7", "R_0603", Some("1k")),
        ("R8", "R_0603", Some("4.7k")),
        ("R9", "R_0603", Some("4.7k")),
        ("R10", "R_0603", Some("100k")),
        ("R11", "R_0603", Some("47k")),
        ("D1", "LED_0603", Some("Green")),
        ("SW1", parts::TACT_ID, Some("RESET")),
        ("SW2", parts::TACT_ID, Some("BOOT")),
        ("J2", "CONN_1x04_P2.54", Some("SHT40 sensor")),
        ("J3", "CONN_1x04_P2.54", Some("SSD1306 OLED")),
        ("H1", parts::MH_ID, None),
        ("H2", parts::MH_ID, None),
        ("H3", parts::MH_ID, None),
        ("H4", parts::MH_ID, None),
    ];
    for (i, (r, comp, val)) in cells.iter().enumerate() {
        let (col, row) = (i % 6, i / 6);
        d.place(r, comp, *val, 30.0 + 45.0 * col as f64, 30.0 + 32.0 * row as f64)?;
    }

    // USB-C input (sink: 5.1 k on CC1/CC2 per USB Type-C rev 2.x Rd).
    d.label("J1", "1", "VBUS", LabelKind::Power)?;
    d.label("J1", "2", "GND", LabelKind::Power)?;
    d.net("J1", "3", "CC1")?;
    d.net("J1", "4", "CC2")?;
    d.net("J1", "5", pins::USB_DP.net)?;
    d.net("J1", "6", pins::USB_DM.net)?;
    d.nc("J1", "7")?;
    d.nc("J1", "8")?;
    d.net("J1", "9", "GND")?;
    d.two("R1", "CC1", "GND")?;
    d.two("R2", "CC2", "GND")?;

    // 3.3 V LDO (AMS1117-3.3: 1 = GND, 2 = VOUT, 3 = VIN).
    d.net("U2", "3", "VBUS")?;
    d.net("U2", "1", "GND")?;
    d.net("U2", "2", "+3V3")?;
    d.two("C1", "VBUS", "GND")?;
    d.two("C2", "+3V3", "GND")?;
    // Module decoupling (datasheet section 8 reference: 10 uF + 0.1 uF at 3V3).
    d.two("C3", "+3V3", "GND")?;
    d.two("C4", "+3V3", "GND")?;

    // ESP32-C3 module.
    d.net("U1", "1", "+3V3")?;
    d.net("U1", "9", "GND")?;
    d.net("U1", "19", "GND")?;
    for a in pins::ALL {
        d.net("U1", a.module_pin, a.net)?;
    }
    for p in pins::UNUSED_MODULE_PINS {
        d.nc("U1", p)?;
    }
    // EN: 10 k pull-up + 1 uF delay (datasheet section 8 note), reset button to GND.
    d.two("R3", "+3V3", pins::CHIP_EN.net)?;
    d.two("C5", pins::CHIP_EN.net, "GND")?;
    d.two("SW1", pins::CHIP_EN.net, "GND")?;
    // GPIO9 BOOT/user button with 10 k pull-up; strapping pull-ups on GPIO8 and GPIO2.
    d.two("R4", "+3V3", pins::USER_BUTTON.net)?;
    d.two("SW2", pins::USER_BUTTON.net, "GND")?;
    d.two("R5", "+3V3", pins::STRAP_GPIO8.net)?;
    d.two("R6", "+3V3", pins::STRAP_GPIO2.net)?;
    // Status LED: GPIO10 -> R7 -> LED anode (pin 2) ; cathode (pin 1) -> GND.
    d.two("R7", pins::STATUS_LED.net, "LED_A")?;
    d.net("D1", "2", "LED_A")?;
    d.net("D1", "1", "GND")?;
    // I2C pull-ups.
    d.two("R8", "+3V3", pins::I2C_SDA.net)?;
    d.two("R9", "+3V3", pins::I2C_SCL.net)?;
    // VBUS sense divider + ADC filter capacitor.
    d.two("R10", "VBUS", pins::VBUS_SENSE.net)?;
    d.two("R11", pins::VBUS_SENSE.net, "GND")?;
    d.two("C6", pins::VBUS_SENSE.net, "GND")?;
    // Headers.
    for (p, n) in pins::J2_SENSOR_HEADER {
        d.net("J2", p, n)?;
    }
    for (p, n) in pins::J3_DISPLAY_HEADER {
        d.net("J3", p, n)?;
    }
    for h in ["H1", "H2", "H3", "H4"] {
        d.net(h, "1", "GND")?;
    }
    Ok(())
}

/// Rules, outline, placement, routing and copper pours.
pub fn build_pcb(d: &mut Driver) -> Result<Vec<String>, String> {
    d.run(C::SetStackup {
        stackup: Stackup::default(),
    })?;
    d.run(C::SetBoardOutline {
        points: vec![
            Point::mm(0.0, 0.0),
            Point::mm(BOARD_W, 0.0),
            Point::mm(BOARD_W, BOARD_H),
            Point::mm(0.0, BOARD_H),
        ],
    })?;
    d.run(C::SetDesignRules {
        min_track_width: Some(150_000),
        min_clearance: Some(150_000),
        min_drill: Some(300_000),
        min_annular_ring: Some(130_000),
        board_edge_clearance: Some(300_000),
        default_track_width: Some(250_000),
        default_clearance: Some(200_000),
        temp_rise_c: Some(10.0),
        max_component_height: Some(12_000_000),
    })?;
    d.run(C::UpsertNetClass {
        class: NetClass {
            name: "Power".into(),
            track_width: 300_000,
            clearance: 200_000,
            via_drill: 300_000,
            via_diameter: 600_000,
            nets: vec!["VBUS".into(), "+3V3".into(), "GND".into()],
        },
    })?;
    d.run(C::UpsertNetClass {
        class: NetClass {
            name: "USB".into(),
            track_width: 200_000,
            clearance: 150_000,
            via_drill: 300_000,
            via_diameter: 600_000,
            nets: vec![pins::USB_DP.net.into(), pins::USB_DM.net.into(), "CC1".into(), "CC2".into()],
        },
    })?;
    for (net, amps, volts) in [
        ("VBUS", 0.5, 5.25),
        ("+3V3", 0.5, 3.3),
        ("GND", 0.5, 0.0),
    ] {
        d.run(C::SetNetCurrent {
            net: net.into(),
            amps: Some(amps),
        })?;
        if volts > 0.0 {
            d.run(C::SetNetVoltage {
                net: net.into(),
                volts: Some(volts),
            })?;
        }
    }
    d.run(C::SyncSchematicToPcb)?;

    // Fixed mechanical / edge parts. Module antenna (local -y) overhangs the bottom edge:
    // the board edge sits 4.5 mm below the module centre line of pad 1.
    d.put("U1", 26.0, 4.5, Rotation::R0)?;
    d.put("J1", 26.0, BOARD_H - 3.2, Rotation::R0)?;
    for (h, x, y) in [
        ("H1", 3.5, 3.5),
        ("H2", BOARD_W - 3.5, 3.5),
        ("H3", 3.5, BOARD_H - 3.5),
        ("H4", BOARD_W - 3.5, BOARD_H - 3.5),
    ] {
        d.put(h, x, y, Rotation::R0)?;
    }
    d.put("J2", 3.0, 22.0, Rotation::R0)?;
    d.put("J3", BOARD_W - 3.0, 22.0, Rotation::R0)?;
    d.put("SW1", 11.0, 12.0, Rotation::R0)?;
    d.put("SW2", 44.0, 11.0, Rotation::R0)?;
    // Power entry next to the USB-C receptacle.
    d.put("U2", 15.0, 33.0, Rotation::R90)?;
    d.put("C1", 19.05, 38.0, Rotation::R0)?;
    d.put("C2", 9.0, 33.0, Rotation::R90)?;
    d.put("R1", 24.75, 30.5, Rotation::R270)?;
    d.put("R2", 31.5, 35.2, Rotation::R0)?;
    // Module decoupling at the 3V3 pin (pad 1 is next to the antenna-side edge).
    d.put("C3", 12.5, 2.0, Rotation::R0)?;
    d.put("C4", 12.5, 5.0, Rotation::R0)?;
    // I2C pull-ups next to the sensor header.
    d.put("R8", 8.5, 21.0, Rotation::R0)?;
    d.put("R9", 8.5, 24.0, Rotation::R0)?;
    // Remaining passives in the open area above the module.
    for (r, x, y) in [
        ("R4", 15.0, 18.0), ("R5", 19.0, 18.0), ("R6", 23.0, 18.0), ("R7", 27.0, 18.0), ("D1", 31.0, 18.0),
        ("R10", 15.0, 22.0), ("R11", 19.0, 22.0), ("C6", 23.0, 22.0),
        ("R3", 15.0, 26.0), ("C5", 19.0, 26.0),
    ] {
        d.put(r, x, y, Rotation::R0)?;
    }

    usb_escape(d, Point::mm(26.0, BOARD_H - 3.2))?;

    let mut notes = vec![];
    let r = d.run(C::AutoRoute { nets: None })?;
    notes.extend(r.warnings);
    for layer in [CopperLayer::Top, CopperLayer::Bottom] {
        d.run(C::AddZone {
            layer,
            net: "GND".into(),
            outline: vec![
                Point::mm(0.5, 0.5),
                Point::mm(BOARD_W - 0.5, 0.5),
                Point::mm(BOARD_W - 0.5, BOARD_H - 0.5),
                Point::mm(0.5, BOARD_H - 0.5),
            ],
            clearance: Some(300_000),
        })?;
    }
    Ok(notes)
}

/// Manual top-layer escape of the 0.5 mm-pitch USB-C lands (EAK's grid router inflates
/// every obstacle by the worst-case clearance + half track width + half a grid cell, which
/// seals 0.2 mm land gaps; see READINESS.md gap G1). Executed as ordinary `AddTrack`
/// commands, exactly as a user would draw them in the PCB editor.
///
/// * D+ (A6, B6) are joined above the land row (under the receptacle body), D- (B7, A7)
///   below it, so the two pairs never cross and no vias are needed.
/// * VBUS (A4B9, B4A9) are joined above the D+ loop.
/// * Every escaping net ends in a stub 1.0 mm apart from its neighbours, from where the
///   auto-router continues.
pub fn usb_escape(d: &mut Driver, center: Point) -> Result<(), String> {
    let at = |x: f64, y: f64| Point::new(center.x + eak_core::geom::mm(x), center.y + eak_core::geom::mm(y));
    let row = -3.4; // land row (local y), lands span row +- 0.575
    let tr = |d: &mut Driver, net: &str, a: Point, b: Point| -> Result<(), String> {
        d.run(C::AddTrack {
            layer: CopperLayer::Top,
            a,
            b,
            width: None,
            net: net.into(),
        })
        .map(|_| ())
    };
    let (dp, dm) = (pins::USB_DP.net, pins::USB_DM.net);
    // D+: A6 (-0.25) and B6 (+0.75) joined at row + 1.35.
    let up = row + 1.35;
    tr(d, dp, at(-0.25, row), at(-0.25, up))?;
    tr(d, dp, at(-0.25, up), at(0.75, up))?;
    tr(d, dp, at(0.75, up), at(0.75, row))?;
    tr(d, dp, at(0.75, row), at(0.75, row - 2.6))?;
    // D-: B7 (-0.75) and A7 (+0.25) joined at row - 1.2, exit at x = -0.25.
    let dn = row - 1.2;
    tr(d, dm, at(-0.75, row), at(-0.75, dn))?;
    tr(d, dm, at(0.25, row), at(0.25, dn))?;
    tr(d, dm, at(-0.75, dn), at(0.25, dn))?;
    tr(d, dm, at(-0.25, dn), at(-0.25, row - 2.6))?;
    // CC1 (A5, -1.25) and CC2 (B5, +1.75) straight down.
    tr(d, "CC1", at(-1.25, row), at(-1.25, row - 2.2))?;
    tr(d, "CC2", at(1.75, row), at(1.75, row - 2.2))?;
    // VBUS: A4B9 (-2.40) and B4A9 (+2.40) joined at row + 2.2, exit downwards from A4B9.
    let vb = row + 2.2;
    tr(d, "VBUS", at(-2.4, row), at(-2.4, vb))?;
    tr(d, "VBUS", at(-2.4, vb), at(2.4, vb))?;
    tr(d, "VBUS", at(2.4, vb), at(2.4, row))?;
    tr(d, "VBUS", at(-2.4, row), at(-2.4, row - 2.0))?;

    // Long runs (board coordinates, mm). The auto-router cannot start from a fine-pitch
    // land, so these nets are routed by hand along fixed corridors.
    let m = Point::mm;
    let path = |d: &mut Driver, net: &str, pts: &[(f64, f64)]| -> Result<(), String> {
        for w in pts.windows(2) {
            tr(d, net, m(w[0].0, w[0].1), m(w[1].0, w[1].1))?;
        }
        Ok(())
    };
    // D- to module pad 13 (IO18), D+ to pad 14 (IO19), down a corridor right of the module.
    path(d, dm, &[(25.75, 34.8), (25.75, 33.6), (38.0, 33.6), (38.0, 8.4), (34.75, 8.4)])?;
    path(d, dp, &[(26.75, 34.8), (26.75, 34.2), (38.6, 34.2), (38.6, 6.9), (34.75, 6.9)])?;
    // CC1 -> R1 pad 1, CC2 -> R2 pad 1 (R1/R2 pad 2 join the GND pour).
    path(d, "CC1", &[(24.75, 35.2), (24.75, 31.325)])?;
    path(d, "CC2", &[(27.75, 35.2), (30.675, 35.2)])?;
    // VBUS -> U2 VIN (pad 3) -> C1 pad 1.
    path(d, "VBUS", &[(23.6, 35.4), (18.15, 35.4), (18.15, 38.0)])?;
    Ok(())
}
