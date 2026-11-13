//! The power-buck board, authored only through `eak_runtime::Engine::execute` with typed
//! `Command`s. This is the same mutation path the EAK desktop UI, CLI and agent use.
//!
//! Coordinates are millimetres, Y up, board origin at the lower-left corner.

use eak_core::{CopperLayer, Id, LabelKind, NetClass, Point, Rotation, Stackup};
use eak_runtime::{Actor, ActorKind, Command as C, CommandRequest, CommandResponse, Engine};
use std::collections::BTreeMap;

pub const BOARD_W: f64 = 66.0;
pub const BOARD_H: f64 = 33.0;
/// The circuit coordinates below were laid out on a 45 mm-high grid; the circuit is shifted
/// down by this amount onto the final 33 mm board. Board-frame items (outline, pours,
/// mounting holes) use `bp()` and are not shifted.
pub const LAYOUT_DY: f64 = -12.0;
pub const COPPER_OZ: f64 = 2.0;
pub const TEMP_RISE_C: f64 = 10.0;

/// Declared worst-case continuous (RMS) current per power net, in amperes.
/// Derivations are in DESIGN.md §3; these drive EAK's IPC-2221 DRC-008 width check.
pub const NET_CURRENTS: &[(&str, f64)] = &[
    ("VIN_RAW", 2.0),
    ("VIN_F", 2.0),
    ("VIN", 2.0),
    ("SW", 3.1),
    ("VOUT", 3.0),
    ("GND", 3.0),
];

/// Declared peak working voltage per net to GND, in volts (DESIGN.md §2).
pub const NET_VOLTAGES: &[(&str, f64)] = &[
    ("VIN_RAW", 25.2),
    ("VIN_F", 25.2),
    ("VIN", 25.2),
    ("SW", 25.2),
    ("BOOT", 30.2),
    ("Q1_G", 13.2),
    ("FB", 0.611),
    ("VOUT", 5.3),
    ("LED_A", 2.5),
    ("GND", 0.0),
];

pub const REQUIREMENTS: &[&str] = &[
    "REQ-IN: input 11.4-25.2 V DC (12 V -5 % to 24 V +5 %) from a SELV / IEC 62368-1 ES1 source; steady-state input above 25.2 V is out of specification",
    "REQ-OUT: output 5.0 V nominal (TPS54302 internal reference, 105k/14.3k divider), 0-3 A continuous, 15 W",
    "REQ-TOPO: non-isolated synchronous buck, TI TPS54302 (integrated 85/40 mOhm FETs, 400 kHz, internal compensation)",
    "REQ-ENV: ambient 0-40 C, natural convection, altitude <= 2000 m, pollution degree 2, no conformal coating",
    "REQ-PROT: 3 A slow-blow input fuse; P-MOSFET reverse-polarity protection; SMAJ26A TVS after the reverse-polarity switch; controller cycle-by-cycle current limit and thermal shutdown",
    "REQ-PCB: 2-layer FR-4 1.6 mm, 2 oz outer copper, conductor temperature rise <= 10 C at declared currents (IPC-2221)",
    "REQ-ISO: no isolation barrier; output 0 V is input 0 V; the board inherits the safety class of its SELV/ES1 source",
];

struct B<'a> {
    e: &'a mut Engine,
    actor: Actor,
    log: Vec<String>,
}

impl B<'_> {
    fn run(&mut self, cmd: C) -> Result<CommandResponse, String> {
        let name = cmd.name();
        let r = self.e.execute(CommandRequest {
            expected_revision: self.e.project().revision,
            actor: self.actor.clone(),
            command: cmd,
            approval: None,
        });
        if !r.accepted {
            return Err(format!("{name} rejected: {:?}", r.error));
        }
        for w in &r.warnings {
            self.log.push(format!("r{} {name}: {w}", r.revision_after));
        }
        Ok(r)
    }
}

fn bp(x: f64, y: f64) -> Point {
    Point::mm(x, y)
}

fn p(x: f64, y: f64) -> Point {
    Point::mm(x, y + LAYOUT_DY)
}

fn mmv(v: f64) -> i64 {
    eak_core::geom::mm(v)
}

/// (refdes, component id, value, schematic position)
const SYMBOLS: &[(&str, &str, &str, (f64, f64))] = &[
    ("J1", "TERM_1x02_P5.08", "VIN 12-24V", (20.0, 150.0)),
    ("F1", "FUSE_1206_3A_SLOW", "3A slow", (50.0, 160.0)),
    ("Q1", "AO4407A", "AO4407A", (80.0, 160.0)),
    ("D1", "MMSZ5242B_SOD-123", "MMSZ5242B 12V", (80.0, 130.0)),
    ("R1", "R_0603", "100k", (110.0, 130.0)),
    ("D2", "SMAJ26A_SMA", "SMAJ26A", (110.0, 160.0)),
    ("C1", "CP_EEE-FK1H470P", "47u 50V", (140.0, 160.0)),
    ("C2", "C_1210_10u_50V_X7R", "10u 50V X7R", (140.0, 130.0)),
    ("C3", "C_1210_10u_50V_X7R", "10u 50V X7R", (140.0, 100.0)),
    ("C4", "C_0603_100n_50V_X7R", "100n 50V X7R", (140.0, 70.0)),
    ("U1", "TPS54302DDCR", "TPS54302DDCR", (180.0, 130.0)),
    ("C5", "C_0603_100n_50V_X7R", "100n 50V X7R", (220.0, 160.0)),
    ("L1", "SRP1265A-100M", "10u", (220.0, 130.0)),
    ("R2", "R_0603", "105k", (220.0, 100.0)),
    ("R3", "R_0603", "14.3k", (220.0, 70.0)),
    ("C6", "C_1210_22u_25V_X7R", "22u 25V X7R", (250.0, 130.0)),
    ("C7", "C_1210_22u_25V_X7R", "22u 25V X7R", (250.0, 100.0)),
    ("R4", "R_0603", "1.5k", (250.0, 70.0)),
    ("D3", "LED_0603", "Green", (250.0, 40.0)),
    ("J2", "TERM_1x02_P5.08", "VOUT 5V", (280.0, 130.0)),
    ("H1", "MH_M3_PTH", "M3", (20.0, 20.0)),
    ("H2", "MH_M3_PTH", "M3", (40.0, 20.0)),
    ("H3", "MH_M3_PTH", "M3", (60.0, 20.0)),
    ("H4", "MH_M3_PTH", "M3", (80.0, 20.0)),
];

/// (refdes, pin, net, power port?)
const CONNECTIONS: &[(&str, &str, &str, bool)] = &[
    // Input connector and fuse.
    ("J1", "1", "VIN_RAW", true),
    ("J1", "2", "GND", true),
    ("F1", "1", "VIN_RAW", false),
    ("F1", "2", "VIN_F", false),
    // Reverse-polarity P-MOSFET: drain from the fuse, source to the protected rail.
    // The protected rail is declared a power port: it is supplied through Q1.
    ("Q1", "3", "VIN_F", false),
    ("Q1", "1", "VIN", true),
    ("Q1", "2", "Q1_G", false),
    // Gate clamp: zener cathode at source, anode at gate; gate pulled to GND by R1.
    ("D1", "1", "VIN", false),
    ("D1", "2", "Q1_G", false),
    ("R1", "1", "Q1_G", false),
    ("R1", "2", "GND", false),
    // TVS and input capacitors.
    ("D2", "1", "VIN", false),
    ("D2", "2", "GND", false),
    ("C1", "1", "VIN", false),
    ("C1", "2", "GND", false),
    ("C2", "2", "VIN", false),
    ("C2", "1", "GND", false),
    ("C3", "2", "VIN", false),
    ("C3", "1", "GND", false),
    ("C4", "2", "VIN", false),
    ("C4", "1", "GND", false),
    // Converter.
    ("U1", "3", "VIN", false),
    ("U1", "1", "GND", false),
    ("U1", "2", "SW", false),
    ("U1", "6", "BOOT", false),
    ("U1", "4", "FB", false),
    ("C5", "1", "BOOT", false),
    ("C5", "2", "SW", false),
    ("L1", "1", "SW", false),
    ("L1", "2", "VOUT", false),
    // Feedback divider: R2 top (VOUT -> FB), R3 bottom (FB -> GND).
    ("R2", "1", "VOUT", false),
    ("R2", "2", "FB", false),
    ("R3", "2", "FB", false),
    ("R3", "1", "GND", false),
    // Output capacitors, power LED and output connector.
    ("C6", "2", "VOUT", false),
    ("C6", "1", "GND", false),
    ("C7", "2", "VOUT", false),
    ("C7", "1", "GND", false),
    ("R4", "1", "VOUT", false),
    ("R4", "2", "LED_A", false),
    ("D3", "2", "LED_A", false),
    ("D3", "1", "GND", false),
    ("J2", "1", "VOUT", false),
    ("J2", "2", "GND", false),
];

/// (refdes, x, y, rotation)
pub const PLACEMENT: &[(&str, f64, f64, Rotation)] = &[
    ("J1", 7.0, 31.0, Rotation::R270),
    ("F1", 15.0, 33.635, Rotation::R0),
    ("Q1", 23.0, 33.0, Rotation::R180),
    ("D1", 28.0, 33.255, Rotation::R90),
    ("R1", 28.0, 37.6, Rotation::R90),
    ("D2", 31.0, 31.7, Rotation::R270),
    ("C1", 24.0, 24.0, Rotation::R180),
    ("C3", 33.7, 26.0, Rotation::R90),
    ("C2", 37.0, 26.0, Rotation::R90),
    ("C4", 39.5, 26.0, Rotation::R90),
    ("U1", 42.0, 26.0, Rotation::R270),
    ("C5", 45.8, 25.5, Rotation::R90),
    ("L1", 46.3, 34.8, Rotation::R0),
    ("R2", 41.05, 21.5, Rotation::R90),
    ("R3", 38.6, 22.325, Rotation::R0),
    ("C6", 51.3, 25.5, Rotation::R90),
    ("C7", 54.6, 25.5, Rotation::R90),
    ("R4", 57.5, 22.0, Rotation::R0),
    ("D3", 61.0, 21.5, Rotation::R180),
    ("J2", 61.0, 30.0, Rotation::R90),
    ("H1", 4.0, 4.0, Rotation::R0),
    ("H2", 62.0, 4.0, Rotation::R0),
    ("H3", 4.0, 29.0, Rotation::R0),
    ("H4", 62.0, 29.0, Rotation::R0),
];

/// Hand-routed copper: (net, layer, width mm, polyline).
/// Every segment is submitted as one `AddTrack` command.
/// (net, width mm, polyline points mm)
pub type Route = (&'static str, f64, Vec<(f64, f64)>);

pub fn tracks() -> Vec<Route> {
    vec![
        // Input: J1.1 -> F1.1 (VIN_RAW), F1.2 -> Q1 drains (VIN_F).
        ("VIN_RAW", 1.0, vec![(7.0, 33.54), (13.55, 33.54)]),
        ("VIN_F", 1.0, vec![(16.45, 33.635), (20.3, 33.635)]),
        ("VIN_F", 1.0, vec![(20.3, 31.095), (20.3, 34.905)]),
        // Q1 sources -> protected VIN rail -> D1 cathode, D2 cathode -> input capacitors.
        ("VIN", 1.0, vec![(25.7, 31.095), (25.7, 33.635)]),
        (
            "VIN",
            1.0,
            vec![(25.7, 32.365), (35.35, 32.365), (35.35, 27.9)],
        ),
        ("VIN", 1.0, vec![(33.7, 27.9), (39.5, 27.9)]),
        ("VIN", 0.8, vec![(39.5, 27.9), (39.5, 26.825)]),
        // Last hop into U1.VIN, narrowed to clear the SW pad by >= 0.35 mm.
        ("VIN", 0.6, vec![(39.5, 27.14), (41.05, 27.14)]),
        // Bulk electrolytic branch.
        ("VIN", 1.0, vec![(27.35, 32.365), (27.35, 24.0)]),
        // Gate network: Q1.G -> D1 anode -> R1.
        (
            "Q1_G",
            0.3,
            vec![(25.7, 34.905), (28.0, 34.905), (28.0, 36.775)],
        ),
        // Switching node: short neck between VIN and GND pads, then 2 mm wide to L1.
        ("SW", 0.85, vec![(42.0, 27.14), (42.0, 29.0)]),
        ("SW", 2.0, vec![(42.0, 29.0), (42.0, 33.5)]),
        ("SW", 0.8, vec![(45.8, 26.325), (45.8, 29.5), (42.0, 29.5)]),
        // Bootstrap.
        ("BOOT", 0.3, vec![(42.95, 24.86), (45.8, 24.86)]),
        // U1 GND pin -> via cluster to the bottom ground plane.
        ("GND", 0.8, vec![(43.2, 27.14), (44.3, 27.14)]),
        ("GND", 0.8, vec![(44.3, 26.1), (44.3, 28.2)]),
        // Output: L1.2 -> C6 -> C7 -> J2.1, LED branch.
        ("VOUT", 1.5, vec![(51.3, 33.0), (51.3, 27.0), (61.0, 27.0)]),
        ("VOUT", 0.8, vec![(56.675, 27.0), (56.675, 22.0)]),
        ("LED_A", 0.3, vec![(58.325, 22.0), (60.175, 21.5)]),
        // Feedback: VOUT sense from C6 to R2 (top), R2/R3 junction to U1.FB.
        (
            "VOUT",
            0.8,
            vec![(51.3, 27.0), (48.5, 27.0), (48.5, 20.675), (41.05, 20.675)],
        ),
        ("FB", 0.3, vec![(41.05, 24.86), (41.05, 22.325)]),
        ("FB", 0.3, vec![(39.425, 22.325), (41.05, 22.325)]),
    ]
}

/// GND vias (0.3 mm drill / 0.6 mm pad) tying top-side ground pads to the bottom plane.
pub const GND_VIAS: &[(f64, f64)] = &[
    // Input capacitors (hot-loop return).
    (33.7, 22.8),
    (35.35, 23.2),
    (36.5, 22.6),
    (39.5, 23.9),
    (38.3, 23.5),
    // U1 GND pin.
    (44.3, 26.1),
    (44.3, 27.15),
    (44.3, 28.2),
    // Output capacitors.
    (49.6, 22.5),
    (51.3, 22.5),
    (52.95, 22.5),
    (54.6, 22.5),
    (55.6, 22.5),
    // TVS, bulk capacitor, gate resistor, LED.
    (31.0, 27.6),
    (20.65, 21.6),
    (17.9, 24.0),
    (28.0, 40.0),
    (63.3, 21.5),
    // Stitching.
    (12.0, 20.0),
    (30.0, 16.0),
    (45.0, 16.0),
    (55.0, 16.0),
    (15.0, 40.0),
    (45.0, 42.0),
];

pub struct Built {
    pub symbols: BTreeMap<String, Id>,
    pub log: Vec<String>,
}

/// Build the complete design. Every mutation goes through `Engine::execute`.
pub fn build(e: &mut Engine) -> Result<Built, String> {
    let mut b = B {
        e,
        actor: Actor {
            kind: ActorKind::Cli,
            name: "validation/power-buck".into(),
        },
        log: vec![],
    };
    b.run(C::SetProjectName {
        name: "EAK Power Buck 12-24V to 5V 3A".into(),
    })?;
    for r in REQUIREMENTS {
        b.run(C::AddRequirement { text: (*r).into() })?;
    }
    // Library: custom definitions via the library-authoring command; built-ins via import.
    for def in crate::parts::all() {
        b.run(C::UpsertComponentDefinition {
            definition: Box::new(def),
        })?;
    }
    for id in ["R_0603", "LED_0603"] {
        b.run(C::ImportLibraryComponent {
            component_id: id.into(),
        })?;
    }

    // ---- schematic
    let mut syms = BTreeMap::new();
    for (refdes, comp, value, (x, y)) in SYMBOLS {
        let r = b.run(C::PlaceSymbol {
            sheet_id: None,
            component_id: (*comp).into(),
            position: p(*x, *y),
            rotation: Rotation::R0,
            refdes: Some((*refdes).into()),
            value: Some((*value).into()),
        })?;
        syms.insert(refdes.to_string(), r.created[0].clone());
    }
    for (refdes, pin, net, power) in CONNECTIONS {
        b.run(C::LabelPin {
            symbol_id: syms[*refdes].clone(),
            pin: (*pin).into(),
            name: (*net).into(),
            kind: if *power {
                LabelKind::Power
            } else {
                LabelKind::Net
            },
        })?;
    }
    // TPS54302 EN: datasheet Table 4-1 "Float the EN pin to enable".
    b.run(C::SetNoConnect {
        symbol_id: syms["U1"].clone(),
        pin: "5".into(),
    })?;

    // ---- board, stackup, rules
    b.run(C::SetBoardOutline {
        points: vec![
            bp(0.0, 0.0),
            bp(BOARD_W, 0.0),
            bp(BOARD_W, BOARD_H),
            bp(0.0, BOARD_H),
        ],
    })?;
    b.run(C::SetStackup {
        stackup: Stackup {
            copper_layers: 2,
            board_thickness: mmv(1.6),
            outer_copper_oz: COPPER_OZ,
            dielectric: "FR-4 (Tg >= 130 C)".into(),
        },
    })?;
    b.run(C::SetDesignRules {
        min_track_width: Some(mmv(0.2)),
        min_clearance: Some(mmv(0.2)),
        min_drill: Some(mmv(0.3)),
        min_annular_ring: Some(mmv(0.15)),
        board_edge_clearance: Some(mmv(0.5)),
        default_track_width: Some(mmv(0.3)),
        default_clearance: Some(mmv(0.2)),
        temp_rise_c: Some(TEMP_RISE_C),
        max_component_height: Some(mmv(12.0)),
    })?;
    b.run(C::UpsertNetClass {
        class: NetClass {
            name: "Power".into(),
            track_width: mmv(0.6),
            clearance: mmv(0.2),
            via_drill: mmv(0.3),
            via_diameter: mmv(0.6),
            nets: ["VIN_RAW", "VIN_F", "VIN", "SW", "VOUT", "GND"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
    })?;
    for (net, a) in NET_CURRENTS {
        b.run(C::SetNetCurrent {
            net: (*net).into(),
            amps: Some(*a),
        })?;
    }
    for (net, v) in NET_VOLTAGES {
        b.run(C::SetNetVoltage {
            net: (*net).into(),
            volts: Some(*v),
        })?;
    }

    // ---- PCB
    b.run(C::SyncSchematicToPcb)?;
    let fp_by_ref: BTreeMap<String, Id> =
        b.e.project()
            .pcb
            .footprints
            .values()
            .map(|f| (f.refdes.clone(), f.id.clone()))
            .collect();
    for (refdes, x, y, rot) in PLACEMENT {
        let id = fp_by_ref
            .get(*refdes)
            .ok_or_else(|| format!("no footprint for {refdes}"))?
            .clone();
        b.run(C::MoveFootprint {
            footprint_id: id.clone(),
            position: if refdes.starts_with('H') {
                bp(*x, *y)
            } else {
                p(*x, *y)
            },
            rotation: Some(*rot),
            side: None,
        })?;
        b.run(C::SetFootprintLocked {
            footprint_id: id,
            locked: true,
        })?;
    }
    for (net, w, pts) in tracks() {
        for seg in pts.windows(2) {
            b.run(C::AddTrack {
                layer: CopperLayer::Top,
                a: p(seg[0].0, seg[0].1),
                b: p(seg[1].0, seg[1].1),
                width: Some(mmv(w)),
                net: net.into(),
            })?;
        }
    }
    for (x, y) in GND_VIAS {
        b.run(C::AddVia {
            position: p(*x, *y),
            net: "GND".into(),
            drill: Some(mmv(0.3)),
            diameter: Some(mmv(0.6)),
        })?;
    }
    // Ground pours: full-board on both layers. 0.6 mm clearance = IPC-2221B B2 for the
    // 30.2 V BOOT node (31-50 V band would apply above 30 V; the declared peak is 30.2 V).
    // Corners are chamfered through the mounting-hole keep-outs so the pour does not leave
    // floating copper slivers between the hole clearance and the board edge.
    let (i, c) = (0.5, 7.0);
    let (w, h) = (BOARD_W, BOARD_H);
    let pour = vec![
        bp(c, i),
        bp(w - c, i),
        bp(w - i, c),
        bp(w - i, h - c),
        bp(w - c, h - i),
        bp(c, h - i),
        bp(i, h - c),
        bp(i, c),
    ];
    for layer in [CopperLayer::Top, CopperLayer::Bottom] {
        b.run(C::AddZone {
            layer,
            net: "GND".into(),
            outline: pour.clone(),
            clearance: Some(mmv(0.6)),
        })?;
    }
    Ok(Built {
        symbols: syms,
        log: b.log,
    })
}
