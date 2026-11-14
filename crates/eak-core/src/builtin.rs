//! Built-in starter library.
//!
//! These definitions are generic land patterns approximating IPC-7351 nominal density
//! and widely published pinouts. Every rating is marked `verified: false` unless a
//! source has been checked; EAK reports unverified ratings as such and never treats them
//! as datasheet facts. Replace or verify before manufacturing use.

use crate::geom::{mm, Point};
use crate::library::*;
use std::collections::BTreeMap;

const GENERIC: &str =
    "EAK built-in generic definition (not verified against a specific manufacturer datasheet)";

fn pin(number: &str, name: &str, t: PinType, x: f64, y: f64) -> SymbolPin {
    SymbolPin {
        number: number.into(),
        name: name.into(),
        electrical_type: t,
        position: Point::mm(x, y),
    }
}

fn smd(number: &str, x: f64, y: f64, w: f64, h: f64) -> PadDef {
    PadDef {
        number: number.into(),
        kind: PadKind::Smd,
        shape: PadShape::Rect,
        position: Point::mm(x, y),
        size_x: mm(w),
        size_y: mm(h),
        drill: None,
    }
}

fn tht(number: &str, x: f64, y: f64, d: f64, drill: f64, square: bool) -> PadDef {
    PadDef {
        number: number.into(),
        kind: PadKind::Tht,
        shape: if square {
            PadShape::Rect
        } else {
            PadShape::Circle
        },
        position: Point::mm(x, y),
        size_x: mm(d),
        size_y: mm(d),
        drill: Some(mm(drill)),
    }
}

fn one_to_one(pins: &[SymbolPin]) -> BTreeMap<String, Vec<String>> {
    pins.iter()
        .map(|p| (p.number.clone(), vec![p.number.clone()]))
        .collect()
}

fn unverified(name: &str, value: f64, unit: &str) -> Rating {
    Rating {
        name: name.into(),
        value,
        unit: unit.into(),
        source: "unverified".into(),
        verified: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn two_pin(
    id: &str,
    name: &str,
    category: &str,
    prefix: &str,
    value: &str,
    pin_names: (&str, &str),
    pad_w: f64,
    pad_h: f64,
    pitch_half: f64,
    court: (f64, f64),
    height: f64,
    model: (&str, &str),
    ratings: Vec<Rating>,
) -> ComponentDef {
    let pins = vec![
        pin("1", pin_names.0, PinType::Passive, -5.08, 0.0),
        pin("2", pin_names.1, PinType::Passive, 5.08, 0.0),
    ];
    ComponentDef {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        refdes_prefix: prefix.into(),
        default_value: value.into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&pins),
        symbol: SymbolDef {
            pins,
            body_half_w: mm(2.54),
            body_half_h: mm(1.0),
        },
        footprint: FootprintDef {
            name: id.into(),
            pads: vec![
                smd("1", -pitch_half, 0.0, pad_w, pad_h),
                smd("2", pitch_half, 0.0, pad_w, pad_h),
            ],
            courtyard_half_w: mm(court.0),
            courtyard_half_h: mm(court.1),
            body_height: Some(mm(height)),
        },
        ratings,
        model3d: Model3d {
            kind: model.0.into(),
            color: model.1.into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: GENERIC.into(),
    }
}

/// Generic two-terminal chip land patterns (IPC-7351 nominal density, approximate):
/// (size code, pad size x, pad size y, pad-centre half pitch, courtyard half extents).
type ChipLand = (&'static str, f64, f64, f64, (f64, f64));
const CHIP_SIZES: [ChipLand; 5] = [
    ("0402", 0.6, 0.6, 0.5, (1.0, 0.55)),
    ("0603", 0.8, 0.95, 0.825, (1.5, 0.75)),
    ("0805", 1.0, 1.45, 0.95, (1.7, 1.0)),
    ("1206", 1.15, 1.8, 1.475, (2.3, 1.15)),
    ("1210", 1.15, 2.7, 1.475, (2.3, 1.6)),
];

fn chip_size(size: &str) -> usize {
    CHIP_SIZES
        .iter()
        .position(|s| s.0 == size)
        .expect("known chip size")
}

/// Generic chip resistor (`R`), MLCC capacitor (`C`) or chip inductor (`L`).
/// Ratings are typical class values, not datasheet facts, and stay unverified.
fn chip(kind: &str, size: &str) -> ComponentDef {
    let i = chip_size(size);
    let (_, w, h, half, court) = CHIP_SIZES[i];
    let (name, category, color, value, height, rating) = match kind {
        "R" => (
            format!("Resistor {size}"),
            "Resistor",
            "#202020",
            "10k",
            [0.35, 0.45, 0.5, 0.6, 0.6][i],
            unverified("power", [0.0625, 0.1, 0.125, 0.25, 0.5][i], "W"),
        ),
        "C" => (
            format!("Capacitor {size} (MLCC)"),
            "Capacitor",
            "#b08a50",
            ["100n", "100n", "10u", "10u", "10u"][i],
            [0.5, 0.8, 1.25, 1.6, 2.5][i],
            unverified("voltage", [16.0, 16.0, 16.0, 25.0, 50.0][i], "V"),
        ),
        "L" => (
            format!("Inductor {size}"),
            "Inductor",
            "#3a3a3a",
            ["1u", "1u", "10u", "10u", "10u"][i],
            [0.5, 0.8, 1.0, 1.1, 1.6][i],
            unverified("current", [0.3, 0.5, 0.8, 1.0, 1.2][i], "A"),
        ),
        _ => unreachable!("chip kind {kind}"),
    };
    two_pin(
        &format!("{kind}_{size}"),
        &name,
        category,
        kind,
        value,
        ("~", "~"),
        w,
        h,
        half,
        court,
        height,
        ("box", color),
        vec![rating],
    )
}

/// Chip LED: pin 1 = cathode, pin 2 = anode.
fn led(size: &str) -> ComponentDef {
    let i = chip_size(size);
    let (_, w, h, half, court) = CHIP_SIZES[i];
    two_pin(
        &format!("LED_{size}"),
        &format!("LED {size}"),
        "LED",
        "D",
        "Green",
        ("K", "A"),
        w,
        h,
        half,
        court,
        [0.4, 0.6, 0.8, 1.1, 1.1][i],
        ("box", "#40c040"),
        vec![unverified("forward_current_max", 0.02, "A")],
    )
}

/// Single-row 2.54 mm pin header, pin 1 square, pins top to bottom.
fn header_1xn(n: usize) -> ComponentDef {
    let top = 1.27 * (n - 1) as f64;
    let pins: Vec<SymbolPin> = (1..=n)
        .map(|i| {
            pin(
                &i.to_string(),
                &format!("P{i}"),
                PinType::Passive,
                -5.08,
                top - 2.54 * (i - 1) as f64,
            )
        })
        .collect();
    ComponentDef {
        id: format!("CONN_1x{n:02}_P2.54"),
        name: format!("Pin header 1x{n:02} 2.54 mm"),
        category: "Connector".into(),
        refdes_prefix: "J".into(),
        default_value: format!("CONN_1x{n:02}"),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&pins),
        symbol: SymbolDef {
            pins,
            body_half_w: mm(2.54),
            body_half_h: mm(1.27 * n as f64),
        },
        footprint: FootprintDef {
            name: format!("PinHeader_1x{n:02}_P2.54mm"),
            pads: (1..=n)
                .map(|i| {
                    tht(
                        &i.to_string(),
                        0.0,
                        top - 2.54 * (i - 1) as f64,
                        1.7,
                        1.0,
                        i == 1,
                    )
                })
                .collect(),
            courtyard_half_w: mm(1.5),
            courtyard_half_h: mm(top + 1.49),
            body_height: Some(mm(8.5)),
        },
        ratings: vec![],
        model3d: Model3d {
            kind: "header".into(),
            color: "#151515".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: GENERIC.into(),
    }
}

/// Three-pin SOT-23 discrete transistor. Pads: 1 and 2 on the left (1 on top), 3 alone on
/// the right. `names` are the pin 1, 2, 3 functions (B/E/C or G/S/D).
fn sot23_transistor(
    id: &str,
    name: &str,
    value: &str,
    names: [&str; 3],
    ratings: Vec<Rating>,
) -> ComponentDef {
    let pins = vec![
        pin("1", names[0], PinType::Passive, -5.08, 0.0),
        pin("2", names[1], PinType::Passive, 2.54, -5.08),
        pin("3", names[2], PinType::Passive, 2.54, 5.08),
    ];
    ComponentDef {
        id: id.into(),
        name: name.into(),
        category: "Transistor".into(),
        refdes_prefix: "Q".into(),
        default_value: value.into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&pins),
        symbol: SymbolDef {
            pins,
            body_half_w: mm(2.54),
            body_half_h: mm(3.81),
        },
        footprint: FootprintDef {
            name: "SOT-23".into(),
            pads: vec![
                smd("1", -1.1, 0.95, 1.3, 0.6),
                smd("2", -1.1, -0.95, 1.3, 0.6),
                smd("3", 1.1, 0.0, 1.3, 0.6),
            ],
            courtyard_half_w: mm(2.05),
            courtyard_half_h: mm(1.7),
            body_height: Some(mm(1.1)),
        },
        ratings,
        model3d: Model3d {
            kind: "box".into(),
            color: "#1a1a1a".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: format!(
            "{GENERIC}; pinout {}/{}/{} is the most common SOT-23 assignment, check the chosen part",
            names[0], names[1], names[2]
        ),
    }
}

pub fn components() -> Vec<ComponentDef> {
    let mut v = vec![chip("R", "0603"), chip("C", "0603"), chip("C", "0805")];

    v.push(led("0603"));

    // 2-pin 2.54 mm header (power input connector).
    let hdr2_pins = vec![
        pin("1", "P1", PinType::Passive, -5.08, 1.27),
        pin("2", "P2", PinType::Passive, -5.08, -1.27),
    ];
    v.push(ComponentDef {
        id: "CONN_1x02_P2.54".into(),
        name: "Pin header 1x02 2.54 mm".into(),
        category: "Connector".into(),
        refdes_prefix: "J".into(),
        default_value: "CONN_1x02".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&hdr2_pins),
        symbol: SymbolDef {
            pins: hdr2_pins,
            body_half_w: mm(2.54),
            body_half_h: mm(2.54),
        },
        footprint: FootprintDef {
            name: "PinHeader_1x02_P2.54mm".into(),
            pads: vec![
                tht("1", 0.0, 1.27, 1.7, 1.0, true),
                tht("2", 0.0, -1.27, 1.7, 1.0, false),
            ],
            courtyard_half_w: mm(1.5),
            courtyard_half_h: mm(2.8),
            body_height: Some(mm(8.5)),
        },
        ratings: vec![unverified("current", 3.0, "A")],
        model3d: Model3d {
            kind: "header".into(),
            color: "#151515".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: GENERIC.into(),
    });

    // 4-pin 2.54 mm header (I/O).
    v.push(header_1xn(4));

    // AMS1117-3.3 LDO in SOT-223. Pinout per the widely published AMS1117 datasheet:
    // 1 = GND/ADJ, 2 = VOUT, 3 = VIN, tab = VOUT.
    let ldo_pins = vec![
        pin("1", "GND", PinType::PowerIn, 0.0, -7.62),
        pin("2", "VOUT", PinType::PowerOut, 7.62, 0.0),
        pin("3", "VIN", PinType::PowerIn, -7.62, 0.0),
    ];
    let mut ldo_map = one_to_one(&ldo_pins);
    ldo_map.insert("2".into(), vec!["2".into(), "4".into()]);
    v.push(ComponentDef {
        id: "AMS1117-3.3_SOT-223".into(),
        name: "AMS1117-3.3 LDO regulator".into(),
        category: "Power".into(),
        refdes_prefix: "U".into(),
        default_value: "AMS1117-3.3".into(),
        manufacturer: Some("Advanced Monolithic Systems".into()),
        mpn: Some("AMS1117-3.3".into()),
        lcsc: Some("C6186".into()),
        datasheet: None,
        pin_map: ldo_map,
        symbol: SymbolDef { pins: ldo_pins, body_half_w: mm(5.08), body_half_h: mm(5.08) },
        footprint: FootprintDef {
            name: "SOT-223-3_TabPin2".into(),
            pads: vec![
                smd("1", -2.3, -3.15, 1.0, 2.0),
                smd("2", 0.0, -3.15, 1.0, 2.0),
                smd("3", 2.3, -3.15, 1.0, 2.0),
                smd("4", 0.0, 3.15, 3.3, 2.0),
            ],
            courtyard_half_w: mm(3.8),
            courtyard_half_h: mm(4.6),
            body_height: Some(mm(1.8)),
        },
        ratings: vec![
            unverified("input_voltage_max", 15.0, "V"),
            unverified("output_current_max", 1.0, "A"),
            unverified("dropout_voltage", 1.3, "V"),
        ],
        model3d: Model3d {
            kind: "box".into(),
            color: "#2a2a2a".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: "Pinout from widely published AMS1117 datasheet; land pattern generic; LCSC part number from public catalogue listing, not re-verified in this build".into(),
    });

    // ATtiny85 in SOIC-8 (pinout per Microchip ATtiny25/45/85 datasheet).
    let mcu_pins = vec![
        pin("1", "PB5/RESET", PinType::Bidirectional, -10.16, 3.81),
        pin("2", "PB3", PinType::Bidirectional, -10.16, 1.27),
        pin("3", "PB4", PinType::Bidirectional, -10.16, -1.27),
        pin("4", "GND", PinType::PowerIn, -10.16, -3.81),
        pin("5", "PB0", PinType::Bidirectional, 10.16, -3.81),
        pin("6", "PB1", PinType::Bidirectional, 10.16, -1.27),
        pin("7", "PB2", PinType::Bidirectional, 10.16, 1.27),
        pin("8", "VCC", PinType::PowerIn, 10.16, 3.81),
    ];
    let soic_pads: Vec<PadDef> = (1..=8)
        .map(|i| {
            let (x, y) = if i <= 4 {
                (-2.7, 1.905 - 1.27 * (i - 1) as f64)
            } else {
                (2.7, -1.905 + 1.27 * (i - 5) as f64)
            };
            smd(&i.to_string(), x, y, 1.55, 0.6)
        })
        .collect();
    v.push(ComponentDef {
        id: "ATTINY85_SOIC-8".into(),
        name: "ATtiny85 microcontroller (SOIC-8)".into(),
        category: "MCU".into(),
        refdes_prefix: "U".into(),
        default_value: "ATtiny85".into(),
        manufacturer: Some("Microchip".into()),
        mpn: Some("ATTINY85-20SU".into()),
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&mcu_pins),
        symbol: SymbolDef { pins: mcu_pins, body_half_w: mm(7.62), body_half_h: mm(6.35) },
        footprint: FootprintDef {
            name: "SOIC-8_3.9x4.9mm_P1.27mm".into(),
            pads: soic_pads,
            courtyard_half_w: mm(3.7),
            courtyard_half_h: mm(2.7),
            body_height: Some(mm(1.75)),
        },
        ratings: vec![unverified("supply_voltage_max", 5.5, "V")],
        model3d: Model3d {
            kind: "box".into(),
            color: "#1a1a1a".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: "Pinout from Microchip ATtiny25/45/85 datasheet (as commonly published); land pattern generic".into(),
    });

    // Generic passives in every common chip size (ids already defined above are kept).
    for kind in ["R", "C", "L"] {
        for (size, ..) in CHIP_SIZES {
            if !v.iter().any(|c| c.id == format!("{kind}_{size}")) {
                v.push(chip(kind, size));
            }
        }
    }
    v.push(led("0805"));

    // Diodes: pin 1 = cathode (band), pin 2 = anode.
    v.push(two_pin(
        "D_SOD-123",
        "Diode SOD-123",
        "Diode",
        "D",
        "Diode",
        ("K", "A"),
        0.9,
        1.2,
        1.65,
        (2.35, 0.95),
        1.2,
        ("box", "#1a1a1a"),
        vec![
            unverified("reverse_voltage_max", 75.0, "V"),
            unverified("forward_current_max", 0.15, "A"),
        ],
    ));
    v.push(two_pin(
        "D_SMA",
        "Diode SMA (DO-214AC)",
        "Diode",
        "D",
        "Schottky",
        ("K", "A"),
        2.2,
        1.8,
        2.05,
        (3.4, 1.55),
        2.3,
        ("box", "#1a1a1a"),
        vec![
            unverified("reverse_voltage_max", 40.0, "V"),
            unverified("forward_current_max", 1.0, "A"),
        ],
    ));

    // SOT-23 discretes. Ratings are small-signal class values, not datasheet facts.
    v.push(sot23_transistor(
        "Q_NPN_SOT-23",
        "NPN BJT (SOT-23)",
        "NPN",
        ["B", "E", "C"],
        vec![
            unverified("vce_max", 40.0, "V"),
            unverified("collector_current_max", 0.2, "A"),
        ],
    ));
    v.push(sot23_transistor(
        "Q_PNP_SOT-23",
        "PNP BJT (SOT-23)",
        "PNP",
        ["B", "E", "C"],
        vec![
            unverified("vce_max", 40.0, "V"),
            unverified("collector_current_max", 0.2, "A"),
        ],
    ));
    v.push(sot23_transistor(
        "Q_NMOS_SOT-23",
        "N-channel MOSFET (SOT-23)",
        "NMOS",
        ["G", "S", "D"],
        vec![
            unverified("vds_max", 60.0, "V"),
            unverified("drain_current_max", 0.2, "A"),
        ],
    ));
    v.push(sot23_transistor(
        "Q_PMOS_SOT-23",
        "P-channel MOSFET (SOT-23)",
        "PMOS",
        ["G", "S", "D"],
        vec![
            unverified("vds_max", 20.0, "V"),
            unverified("drain_current_max", 0.5, "A"),
        ],
    ));

    // SOT-23-5 LDO. Pins 1-3 on the bottom row left to right, 4-5 on the top row right to
    // left. Pinout 1 VIN, 2 GND, 3 EN, 4 NC, 5 VOUT is the common assignment of this
    // regulator class; some parts use pin 4 as BYP, so check the chosen part.
    let ldo5_pins = vec![
        pin("1", "VIN", PinType::PowerIn, -7.62, 2.54),
        pin("2", "GND", PinType::PowerIn, 0.0, -7.62),
        pin("3", "EN", PinType::Input, -7.62, -2.54),
        pin("4", "NC", PinType::NoConnect, 7.62, -2.54),
        pin("5", "VOUT", PinType::PowerOut, 7.62, 2.54),
    ];
    v.push(ComponentDef {
        id: "LDO_SOT-23-5".into(),
        name: "LDO regulator (SOT-23-5, VIN/GND/EN/NC/VOUT)".into(),
        category: "Power".into(),
        refdes_prefix: "U".into(),
        default_value: "LDO_3.3V".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&ldo5_pins),
        symbol: SymbolDef {
            pins: ldo5_pins,
            body_half_w: mm(5.08),
            body_half_h: mm(5.08),
        },
        footprint: FootprintDef {
            name: "SOT-23-5".into(),
            pads: vec![
                smd("1", -0.95, -1.1, 0.6, 1.1),
                smd("2", 0.0, -1.1, 0.6, 1.1),
                smd("3", 0.95, -1.1, 0.6, 1.1),
                smd("4", 0.95, 1.1, 0.6, 1.1),
                smd("5", -0.95, 1.1, 0.6, 1.1),
            ],
            courtyard_half_w: mm(1.75),
            courtyard_half_h: mm(1.9),
            body_height: Some(mm(1.1)),
        },
        ratings: vec![
            unverified("input_voltage_max", 6.0, "V"),
            unverified("output_current_max", 0.3, "A"),
            unverified("dropout_voltage", 0.25, "V"),
        ],
        model3d: Model3d {
            kind: "box".into(),
            color: "#1a1a1a".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: format!(
            "{GENERIC}; pinout is the common SOT-23-5 LDO assignment, check the chosen part"
        ),
    });

    // Remaining single-row headers (1x02 and 1x04 are defined above).
    for n in [3, 5, 6, 7, 8, 9, 10] {
        v.push(header_1xn(n));
    }

    // Crystal, HC-49/US through-hole: two leads at 4.88 mm pitch, body 11 x 4.65 mm.
    let xtal_pins = vec![
        pin("1", "1", PinType::Passive, -5.08, 0.0),
        pin("2", "2", PinType::Passive, 5.08, 0.0),
    ];
    v.push(ComponentDef {
        id: "XTAL_HC-49US".into(),
        name: "Crystal HC-49/US".into(),
        category: "Crystal".into(),
        refdes_prefix: "Y".into(),
        default_value: "16MHz".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&xtal_pins),
        symbol: SymbolDef {
            pins: xtal_pins,
            body_half_w: mm(2.54),
            body_half_h: mm(1.27),
        },
        footprint: FootprintDef {
            name: "Crystal_HC49-US_P4.88mm".into(),
            pads: vec![
                tht("1", -2.44, 0.0, 1.5, 0.8, false),
                tht("2", 2.44, 0.0, 1.5, 0.8, false),
            ],
            courtyard_half_w: mm(5.8),
            courtyard_half_h: mm(2.6),
            body_height: Some(mm(3.6)),
        },
        ratings: vec![unverified("load_capacitance", 20e-12, "F")],
        model3d: Model3d {
            kind: "box".into(),
            color: "#c0c0c0".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: GENERIC.into(),
    });

    // Crystal, 3.2 x 2.5 mm four-pad SMD: pads 1 and 3 (diagonal) are the crystal, 2 and 4
    // the case/ground, numbered counter-clockwise from bottom left (common layout; check
    // the chosen part).
    let xtal4_pins = vec![
        pin("1", "1", PinType::Passive, -5.08, 0.0),
        pin("2", "GND", PinType::Passive, 0.0, -5.08),
        pin("3", "3", PinType::Passive, 5.08, 0.0),
    ];
    let mut xtal4_map = one_to_one(&xtal4_pins);
    xtal4_map.insert("2".into(), vec!["2".into(), "4".into()]);
    v.push(ComponentDef {
        id: "XTAL_3225".into(),
        name: "Crystal 3.2x2.5 mm, 4-pad".into(),
        category: "Crystal".into(),
        refdes_prefix: "Y".into(),
        default_value: "16MHz".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: xtal4_map,
        symbol: SymbolDef {
            pins: xtal4_pins,
            body_half_w: mm(2.54),
            body_half_h: mm(2.54),
        },
        footprint: FootprintDef {
            name: "Crystal_SMD_3225-4Pin".into(),
            pads: vec![
                smd("1", -1.1, -0.8, 1.4, 1.2),
                smd("2", 1.1, -0.8, 1.4, 1.2),
                smd("3", 1.1, 0.8, 1.4, 1.2),
                smd("4", -1.1, 0.8, 1.4, 1.2),
            ],
            courtyard_half_w: mm(2.05),
            courtyard_half_h: mm(1.65),
            body_height: Some(mm(0.8)),
        },
        ratings: vec![unverified("load_capacitance", 12e-12, "F")],
        model3d: Model3d {
            kind: "box".into(),
            color: "#c0c0c0".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: format!("{GENERIC}; pad assignment is the common 3225 4-pad layout"),
    });

    // 6 x 6 mm through-hole tactile switch. Leads 6.5 mm apart (1-2, 3-4) are joined
    // inside the switch; pressing connects pin 1 to pin 2.
    let sw_pins = vec![
        pin("1", "1", PinType::Passive, -5.08, 0.0),
        pin("2", "2", PinType::Passive, 5.08, 0.0),
    ];
    let mut sw_map = BTreeMap::new();
    sw_map.insert("1".to_string(), vec!["1".to_string(), "2".to_string()]);
    sw_map.insert("2".to_string(), vec!["3".to_string(), "4".to_string()]);
    v.push(ComponentDef {
        id: "SW_Tactile_6x6_THT".into(),
        name: "Tactile switch 6x6 mm (THT)".into(),
        category: "Switch".into(),
        refdes_prefix: "SW".into(),
        default_value: "SW_Push".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: sw_map,
        symbol: SymbolDef {
            pins: sw_pins,
            body_half_w: mm(2.54),
            body_half_h: mm(1.27),
        },
        footprint: FootprintDef {
            name: "SW_Tactile_6x6mm_THT".into(),
            pads: vec![
                tht("1", -3.25, 2.25, 2.0, 1.0, true),
                tht("2", 3.25, 2.25, 2.0, 1.0, false),
                tht("3", -3.25, -2.25, 2.0, 1.0, false),
                tht("4", 3.25, -2.25, 2.0, 1.0, false),
            ],
            courtyard_half_w: mm(4.5),
            courtyard_half_h: mm(3.5),
            body_height: Some(mm(5.0)),
        },
        ratings: vec![
            unverified("voltage_max", 12.0, "V"),
            unverified("current_max", 0.05, "A"),
        ],
        model3d: Model3d {
            kind: "box".into(),
            color: "#303030".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: format!(
            "{GENERIC}; internal pairing is the common 6x6 mm tactile arrangement, check the chosen part"
        ),
    });

    v.extend(board_parts());

    // shortcut: no USB-C receptacle; its shell and locating pegs need slotted / non-plated
    // holes that FootprintDef cannot express. Add one when the footprint model gains them.
    v
}

/// A rating whose value was read from the named datasheet. It stays `verified: false`:
/// EAK has not independently reviewed the source.
fn datasheet_rating(name: &str, value: f64, unit: &str, source: &str) -> Rating {
    Rating {
        name: name.into(),
        value,
        unit: unit.into(),
        source: source.into(),
        verified: false,
    }
}

/// Lays symbol pins out top to bottom on one side at 2.54 mm spacing.
fn column(pins: &[(&str, &str, PinType)], x: f64, top: f64) -> Vec<SymbolPin> {
    pins.iter()
        .enumerate()
        .map(|(i, (n, name, t))| pin(n, name, *t, x, top - 2.54 * i as f64))
        .collect()
}

/// SOIC-8 / MS-012 pads (1.55 x 0.6 at 1.27 mm pitch, rows 5.4 mm apart), pin 1 top left.
fn soic8_pads() -> Vec<PadDef> {
    (1..=8)
        .map(|i| {
            let (x, y) = if i <= 4 {
                (-2.7, 1.905 - 1.27 * (i - 1) as f64)
            } else {
                (2.7, -1.905 + 1.27 * (i - 5) as f64)
            };
            smd(&i.to_string(), x, y, 1.55, 0.6)
        })
        .collect()
}

fn model(color: &str) -> Model3d {
    Model3d {
        kind: "box".into(),
        color: color.into(),
        external_ref: None,
        model_ref: None,
    }
}

const ESP32_C3_MINI_1_DS: &str =
    "https://www.espressif.com/sites/default/files/documentation/esp32-c3-mini-1_datasheet_en.pdf";
const LMR33630_DS: &str = "https://www.ti.com/lit/ds/symlink/lmr33630.pdf";
const SHT4X_DS: &str = "https://sensirion.com/resource/datasheet/sht4x";
const AO4406A_DS: &str = "https://www.aosmd.com/res/datasheets/AO4406A.pdf";
const SRP1038A_DS: &str = "https://www.bourns.com/docs/Product-Datasheets/SRP1038A.pdf";
const WE_74477710_DS: &str = "https://www.we-online.com/components/products/datasheet/74477710.pdf";
const WE_74404054100_DS: &str =
    "https://www.we-online.com/components/products/datasheet/74404054100.pdf";
const PANASONIC_SMD_ECAP_LAND: &str =
    "https://industrial.panasonic.com/cdbs/www-data/pdf/RDE0000/ABA0000C1247.pdf";
const MKDS_1715721_DS: &str = "Phoenix Contact 1715721 (MKDS 1,5/ 2-5,08) catalogue PDF, Feb 2022";

/// Parts with manufacturer pinouts and land patterns, for the ESP32-C3 dashboard board and
/// the 12-24 V to 5 V buck board.
fn board_parts() -> Vec<ComponentDef> {
    let mut v = vec![];

    // ESP32-C3-MINI-1: datasheet v2.2 Table 3-1 pins, Fig. 11-1 recommended land pattern.
    // Origin is the module centre; the pad field centre sits 2.7 mm below it (antenna end
    // at +Y). GND pads 1, 2, 11, 14, 36-53 all join symbol pin 1; the 3x3 centre GND array
    // is pin 49 (pads 49a-49i).
    {
        use PinType::*;
        let dy = -2.7;
        let mut pads = vec![];
        for n in 1..=11 {
            let y = 4.0 - 0.8 * (n - 1) as f64 + dy;
            pads.push(smd(&n.to_string(), -5.9, y, 0.8, 0.4));
        }
        for n in 12..=24 {
            let x = -4.8 + 0.8 * (n - 12) as f64;
            pads.push(smd(&n.to_string(), x, -4.9 + dy, 0.4, 0.8));
        }
        for n in 25..=35 {
            let y = -4.0 + 0.8 * (n - 25) as f64 + dy;
            pads.push(smd(&n.to_string(), 5.9, y, 0.8, 0.4));
        }
        for n in 36..=48 {
            let x = 4.8 - 0.8 * (n - 36) as f64;
            pads.push(smd(&n.to_string(), x, 4.9 + dy, 0.4, 0.8));
        }
        for (n, x, y) in [
            ("50", 5.95, 4.95),
            ("51", 5.95, -4.95),
            ("52", -5.95, -4.95),
            ("53", -5.95, 4.95),
        ] {
            pads.push(smd(n, x, y + dy, 0.7, 0.7));
        }
        let mut gnd_pads: Vec<String> = [1, 2, 11, 14]
            .into_iter()
            .chain(36..=48)
            .chain(50..=53)
            .map(|n| n.to_string())
            .collect();
        let grid = [-1.975, 0.0, 1.975];
        for (k, (x, y)) in grid.iter().flat_map(|y| grid.map(|x| (x, *y))).enumerate() {
            let num = format!("49{}", char::from(b'a' + k as u8));
            pads.push(smd(&num, x, y + dy, 1.45, 1.45));
            gnd_pads.push(num);
        }

        let left = [
            ("3", "3V3", PowerIn),
            ("8", "EN", Input),
            ("1", "GND", PowerIn),
            ("4", "NC", NoConnect),
            ("7", "NC", NoConnect),
            ("9", "NC", NoConnect),
            ("10", "NC", NoConnect),
            ("15", "NC", NoConnect),
            ("17", "NC", NoConnect),
            ("24", "NC", NoConnect),
            ("25", "NC", NoConnect),
            ("28", "NC", NoConnect),
            ("29", "NC", NoConnect),
            ("32", "NC", NoConnect),
            ("33", "NC", NoConnect),
            ("34", "NC", NoConnect),
            ("35", "NC", NoConnect),
        ];
        let right = [
            ("12", "IO0", Bidirectional),
            ("13", "IO1", Bidirectional),
            ("5", "IO2", Bidirectional),
            ("6", "IO3", Bidirectional),
            ("18", "IO4", Bidirectional),
            ("19", "IO5", Bidirectional),
            ("20", "IO6", Bidirectional),
            ("21", "IO7", Bidirectional),
            ("22", "IO8", Bidirectional),
            ("23", "IO9", Bidirectional),
            ("16", "IO10", Bidirectional),
            ("26", "IO18/USB_D-", Bidirectional),
            ("27", "IO19/USB_D+", Bidirectional),
            ("30", "RXD0", Bidirectional),
            ("31", "TXD0", Bidirectional),
        ];
        let mut pins = column(&left, -12.7, 20.32);
        pins.extend(column(&right, 12.7, 20.32));
        let mut pin_map = one_to_one(&pins);
        pin_map.insert("1".into(), gnd_pads);
        v.push(ComponentDef {
            id: "ESP32-C3-MINI-1".into(),
            name: "ESP32-C3-MINI-1 Wi-Fi/BLE module (PCB antenna)".into(),
            category: "MCU".into(),
            refdes_prefix: "U".into(),
            default_value: "ESP32-C3-MINI-1".into(),
            manufacturer: Some("Espressif Systems".into()),
            mpn: Some("ESP32-C3-MINI-1".into()),
            lcsc: None,
            datasheet: Some(ESP32_C3_MINI_1_DS.into()),
            pin_map,
            symbol: SymbolDef {
                pins,
                body_half_w: mm(10.16),
                body_half_h: mm(22.86),
            },
            footprint: FootprintDef {
                name: "ESP32-C3-MINI-1".into(),
                pads,
                courtyard_half_w: mm(6.85),
                courtyard_half_h: mm(8.55),
                body_height: Some(mm(2.4)),
            },
            ratings: vec![
                datasheet_rating("supply_voltage_min", 3.0, "V", ESP32_C3_MINI_1_DS),
                datasheet_rating("supply_voltage_max", 3.6, "V", ESP32_C3_MINI_1_DS),
            ],
            model3d: model("#c8c8c8"),
            provenance: format!(
                "Pinout from Espressif ESP32-C3-MINI-1 datasheet v2.2 Table 3-1; land pattern \
                 from its Fig. 11-1 (pad positions partly scaled from the drawing, about \
                 +/-0.05 mm). The antenna is the top 5.4 mm of the module (+Y): keep copper, \
                 tracks and parts off every layer under it (not modelled as a keep-out here). \
                 Source: {ESP32_C3_MINI_1_DS}"
            ),
        });
    }

    // TI LMR33630 3.8-36 V, 3 A synchronous buck, HSOIC-8 (DDA) PowerPAD: Table 6-1 pins,
    // example board layout (SOIC-8 pads, thermal copper 2.95 x 4.9 mm as pad 9 = AGND).
    {
        use PinType::*;
        let left = [
            ("1", "PGND", PowerIn),
            ("2", "VIN", PowerIn),
            ("3", "EN", Input),
            ("4", "PG", OpenCollector),
        ];
        let right = [
            ("8", "SW", Output),
            ("7", "BOOT", Passive),
            ("6", "VCC", PowerOut),
            ("5", "FB", Input),
        ];
        let mut pins = column(&left, -10.16, 3.81);
        pins.extend(column(&right, 10.16, 3.81));
        pins.push(pin("9", "AGND", PowerIn, 0.0, -7.62));
        let mut pads = soic8_pads();
        pads.push(smd("9", 0.0, 0.0, 2.95, 4.9));
        v.push(ComponentDef {
            id: "LMR33630_HSOIC-8".into(),
            name: "LMR33630 3 A synchronous buck converter (HSOIC-8 PowerPAD)".into(),
            category: "Power".into(),
            refdes_prefix: "U".into(),
            default_value: "LMR33630ADDA".into(),
            manufacturer: Some("Texas Instruments".into()),
            mpn: Some("LMR33630ADDA".into()),
            lcsc: None,
            datasheet: Some(LMR33630_DS.into()),
            pin_map: one_to_one(&pins),
            symbol: SymbolDef {
                pins,
                body_half_w: mm(7.62),
                body_half_h: mm(6.35),
            },
            footprint: FootprintDef {
                name: "HSOIC-8_3.9x4.9mm_P1.27mm_EP2.95x4.9mm".into(),
                pads,
                courtyard_half_w: mm(3.75),
                courtyard_half_h: mm(2.75),
                body_height: Some(mm(1.7)),
            },
            ratings: vec![
                datasheet_rating("input_voltage_max", 36.0, "V", LMR33630_DS),
                datasheet_rating("input_voltage_abs_max", 38.0, "V", LMR33630_DS),
                datasheet_rating("output_current_max", 3.0, "A", LMR33630_DS),
                datasheet_rating("high_side_current_limit_max", 5.05, "A", LMR33630_DS),
            ],
            model3d: model("#1a1a1a"),
            provenance: format!(
                "Pinout from TI LMR33630 datasheet SNVSAN3F Table 6-1; land pattern from its \
                 DDA example board layout. Thermal pad copper covers both the DDA0008J and \
                 DDA0008B outlines; the paste window (2.71 x 3.4) is not modelled. \
                 Source: {LMR33630_DS}"
            ),
        });
    }

    // Sensirion SHT40 humidity/temperature sensor, DFN-4 1.5 x 1.5 mm. Sensirion
    // recommends no centre pad and no copper under the sensor, so none is defined.
    {
        let pins = vec![
            pin("1", "SDA", PinType::Bidirectional, -7.62, 1.27),
            pin("2", "SCL", PinType::Input, -7.62, -1.27),
            pin("3", "VDD", PinType::PowerIn, 7.62, 1.27),
            pin("4", "VSS", PinType::PowerIn, 7.62, -1.27),
        ];
        v.push(ComponentDef {
            id: "SHT40_DFN-4".into(),
            name: "SHT40 humidity and temperature sensor (I2C, DFN-4)".into(),
            category: "Sensor".into(),
            refdes_prefix: "U".into(),
            default_value: "SHT40-AD1B".into(),
            manufacturer: Some("Sensirion".into()),
            mpn: Some("SHT40-AD1B".into()),
            lcsc: None,
            datasheet: Some(SHT4X_DS.into()),
            pin_map: one_to_one(&pins),
            symbol: SymbolDef {
                pins,
                body_half_w: mm(5.08),
                body_half_h: mm(3.81),
            },
            footprint: FootprintDef {
                name: "Sensirion_DFN-4_1.5x1.5mm_P0.8mm".into(),
                pads: vec![
                    smd("1", -0.7, 0.4, 0.5, 0.3),
                    smd("2", -0.7, -0.4, 0.5, 0.3),
                    smd("3", 0.7, -0.4, 0.5, 0.3),
                    smd("4", 0.7, 0.4, 0.5, 0.3),
                ],
                courtyard_half_w: mm(1.2),
                courtyard_half_h: mm(1.0),
                body_height: Some(mm(0.54)),
            },
            ratings: vec![
                datasheet_rating("supply_voltage_min", 1.08, "V", SHT4X_DS),
                datasheet_rating("supply_voltage_max", 3.6, "V", SHT4X_DS),
            ],
            model3d: model("#d0d0d0"),
            provenance: format!(
                "Pinout and land pattern from Sensirion SHT4x datasheet v7.3 (sections 5.3, \
                 5.4). I2C address 0x44 for SHT40-AD1B (B = 0x45, C = 0x46). No copper under \
                 the body other than the pads (not modelled as a keep-out). Source: {SHT4X_DS}"
            ),
        });
    }

    // AOS AO4406A 30 V N-channel MOSFET, SO-8: S = 1-3, G = 4, D = 5-8. AOS gives no land
    // pattern, so the generic MS-012 one is used.
    {
        let pins = vec![
            pin("1", "S", PinType::Passive, 2.54, -5.08),
            pin("4", "G", PinType::Passive, -5.08, 0.0),
            pin("5", "D", PinType::Passive, 2.54, 5.08),
        ];
        let pads_of = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let pin_map = BTreeMap::from([
            ("1".to_string(), pads_of(&["1", "2", "3"])),
            ("4".to_string(), pads_of(&["4"])),
            ("5".to_string(), pads_of(&["5", "6", "7", "8"])),
        ]);
        v.push(ComponentDef {
            id: "AO4406A_SO-8".into(),
            name: "AO4406A 30 V N-channel MOSFET (SO-8)".into(),
            category: "Transistor".into(),
            refdes_prefix: "Q".into(),
            default_value: "AO4406A".into(),
            manufacturer: Some("Alpha & Omega Semiconductor".into()),
            mpn: Some("AO4406A".into()),
            lcsc: None,
            datasheet: Some(AO4406A_DS.into()),
            pin_map,
            symbol: SymbolDef {
                pins,
                body_half_w: mm(2.54),
                body_half_h: mm(3.81),
            },
            footprint: FootprintDef {
                name: "SOIC-8_3.9x4.9mm_P1.27mm".into(),
                pads: soic8_pads(),
                courtyard_half_w: mm(3.7),
                courtyard_half_h: mm(2.7),
                body_height: Some(mm(1.75)),
            },
            ratings: vec![
                datasheet_rating("vds_max", 30.0, "V", AO4406A_DS),
                datasheet_rating("vgs_max", 20.0, "V", AO4406A_DS),
                datasheet_rating("drain_current_max", 13.0, "A", AO4406A_DS),
            ],
            model3d: model("#1a1a1a"),
            provenance: format!(
                "Ratings and S,S,S,G / D,D,D,D lead labels from AOS AO4406A datasheet Rev 3.0; \
                 pin numbers (1-3 S, 4 G, 5-8 D) are the standard single-N SO-8 assignment, \
                 not read from the drawing; land pattern generic MS-012. Source: {AO4406A_DS}"
            ),
        });
    }

    // Shielded power inductors, land patterns from the order-code datasheets. Pads along X.
    // (id, name, manufacturer, mpn, datasheet, pad size, pad half pitch, courtyard,
    //  height, rated current, saturation current)
    type Inductor<'a> = (
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        (f64, f64),
        f64,
        (f64, f64),
        f64,
        f64,
        f64,
    );
    let inductors: [Inductor; 3] = [
        (
            "L_SRP1038A",
            "Power inductor 10 uH, Bourns SRP1038A (11 x 10 mm, shielded)",
            "Bourns",
            "SRP1038A-100M",
            SRP1038A_DS,
            (4.1, 3.5),
            4.75,
            (7.05, 5.25),
            4.0,
            7.5,
            12.0,
        ),
        (
            "L_WE-PD_7345",
            "Power inductor 10 uH, Würth WE-PD 7345 (7.3 x 7.3 mm, shielded)",
            "Würth Elektronik",
            "74477710",
            WE_74477710_DS,
            (1.6, 2.2),
            3.2,
            (4.25, 3.9),
            4.5,
            2.0,
            2.6,
        ),
        (
            "L_WE-LQS_5040",
            "Power inductor 10 uH, Würth WE-LQS 5040 (5 x 5 mm, semi-shielded)",
            "Würth Elektronik",
            "74404054100",
            WE_74404054100_DS,
            (1.55, 4.5),
            1.925,
            (2.95, 2.75),
            4.0,
            2.1,
            2.5,
        ),
    ];
    for (id, name, maker, mpn, ds, pad, half, court, height, ir, isat) in inductors {
        let mut c = two_pin(
            id,
            name,
            "Inductor",
            "L",
            "10u",
            ("~", "~"),
            pad.0,
            pad.1,
            half,
            court,
            height,
            ("box", "#3a3a3a"),
            vec![
                datasheet_rating("current", ir, "A", ds),
                datasheet_rating("saturation_current", isat, "A", ds),
            ],
        );
        c.manufacturer = Some(maker.into());
        c.mpn = Some(mpn.into());
        c.datasheet = Some(ds.into());
        c.provenance = format!(
            "Land pattern, rated and saturation current from {maker} {mpn} datasheet; check \
             saturation current against the regulator's peak current limit. Source: {ds}"
        );
        v.push(c);
    }

    // SMD aluminium electrolytic capacitors; pads from the Panasonic land table (cases D
    // and F). Pad 1 = + is an EAK convention; the can's ink band marks the - side.
    for (id, name, value, pad, half, court, height, volts) in [
        (
            "CP_Elec_6.3x5.4",
            "Electrolytic capacitor SMD 6.3 x 5.4 mm (case D)",
            "47u",
            (3.2, 1.6),
            2.5,
            (4.35, 3.55),
            5.8,
            35.0,
        ),
        (
            "CP_Elec_8x10.2",
            "Electrolytic capacitor SMD 8 x 10.2 mm (case F)",
            "100u",
            (4.0, 2.0),
            3.55,
            (5.8, 4.4),
            10.5,
            50.0,
        ),
    ] {
        let mut c = two_pin(
            id,
            name,
            "Capacitor",
            "C",
            value,
            ("+", "-"),
            pad.0,
            pad.1,
            half,
            court,
            height,
            ("cylinder", "#303060"),
            vec![unverified("voltage", volts, "V")],
        );
        c.provenance = format!(
            "Land pattern from the Panasonic SMD aluminium electrolytic land table \
             ({PANASONIC_SMD_ECAP_LAND}); body size and voltage are generic class values; \
             pad 1 = + is an EAK convention"
        );
        v.push(c);
    }

    // Phoenix Contact MKDS 1,5/ 2-5,08 screw terminal: 5.08 mm pitch, 1.3 mm holes, body
    // 10.16 x 9.8 mm with the hole row 4.6 mm from one edge. Origin is the body centre.
    {
        let pins = vec![
            pin("1", "1", PinType::Passive, -5.08, 1.27),
            pin("2", "2", PinType::Passive, -5.08, -1.27),
        ];
        v.push(ComponentDef {
            id: "TERM_1x02_P5.08".into(),
            name: "Screw terminal 2-pin 5.08 mm (Phoenix MKDS 1,5/ 2-5,08)".into(),
            category: "Connector".into(),
            refdes_prefix: "J".into(),
            default_value: "MKDS 1,5/ 2-5,08".into(),
            manufacturer: Some("Phoenix Contact".into()),
            mpn: Some("1715721".into()),
            lcsc: None,
            datasheet: Some(MKDS_1715721_DS.into()),
            pin_map: one_to_one(&pins),
            symbol: SymbolDef {
                pins,
                body_half_w: mm(2.54),
                body_half_h: mm(2.54),
            },
            footprint: FootprintDef {
                name: "TerminalBlock_Phoenix_MKDS-1,5-2-5.08".into(),
                pads: vec![
                    tht("1", -2.54, -0.3, 2.4, 1.3, true),
                    tht("2", 2.54, -0.3, 2.4, 1.3, false),
                ],
                courtyard_half_w: mm(5.33),
                courtyard_half_h: mm(5.15),
                body_height: Some(mm(13.8)),
            },
            ratings: vec![
                datasheet_rating("current", 15.0, "A", MKDS_1715721_DS),
                datasheet_rating("voltage", 300.0, "V", MKDS_1715721_DS),
            ],
            model3d: model("#2f8f3f"),
            provenance: format!(
                "Drill, pitch and body from {MKDS_1715721_DS}; ratings are the cULus values \
                 (300 V / 15 A); pad diameter is EAK engineering practice"
            ),
        });
    }

    v
}

pub fn get(id: &str) -> Option<ComponentDef> {
    components().into_iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Nm;
    use std::collections::BTreeSet;

    /// Pad copper bounding box (circles use their bounding square).
    fn bbox(p: &PadDef) -> (Nm, Nm, Nm, Nm) {
        (
            p.position.x - p.size_x / 2,
            p.position.y - p.size_y / 2,
            p.position.x + p.size_x / 2,
            p.position.y + p.size_y / 2,
        )
    }

    #[test]
    fn builtin_library_is_structurally_valid() {
        let all = components();
        let mut ids = BTreeSet::new();
        for c in &all {
            assert!(ids.insert(c.id.clone()), "duplicate id {}", c.id);
            let problems = c.validate();
            assert!(problems.is_empty(), "{}: {problems:?}", c.id);

            // Every symbol pin (including no-connects) maps to at least one pad, and every
            // pad is reached from some pin.
            let mut mapped = BTreeSet::new();
            for pin in &c.symbol.pins {
                let pads = c.pin_map.get(&pin.number);
                assert!(
                    pads.is_some_and(|p| !p.is_empty()),
                    "{}: pin {} has no pad",
                    c.id,
                    pin.number
                );
                mapped.extend(pads.unwrap().iter().cloned());
            }
            for pad in &c.footprint.pads {
                assert!(
                    mapped.contains(&pad.number),
                    "{}: pad {} is not mapped from any pin",
                    c.id,
                    pad.number
                );
            }

            // No two pads overlap, and the courtyard contains every pad.
            let fp = &c.footprint;
            for (i, a) in fp.pads.iter().enumerate() {
                let (ax0, ay0, ax1, ay1) = bbox(a);
                assert!(
                    ax0 >= -fp.courtyard_half_w
                        && ax1 <= fp.courtyard_half_w
                        && ay0 >= -fp.courtyard_half_h
                        && ay1 <= fp.courtyard_half_h,
                    "{}: pad {} outside courtyard",
                    c.id,
                    a.number
                );
                for b in &fp.pads[i + 1..] {
                    let (bx0, by0, bx1, by1) = bbox(b);
                    let apart = ax1 <= bx0 || bx1 <= ax0 || ay1 <= by0 || by1 <= ay0;
                    assert!(
                        apart,
                        "{}: pads {} and {} overlap",
                        c.id, a.number, b.number
                    );
                }
            }
            assert!(
                fp.body_height.is_some_and(|h| h > 0),
                "{}: no body height",
                c.id
            );

            // No rating is presented as verified, and each names where it came from.
            for r in &c.ratings {
                assert!(
                    !r.verified && !r.source.is_empty(),
                    "{}: rating {} claims verification",
                    c.id,
                    r.name
                );
            }
        }
    }

    #[test]
    fn original_m1_parts_are_unchanged() {
        let golden: Vec<ComponentDef> =
            serde_json::from_str(include_str!("../tests/fixtures/builtin_m1_parts.json"))
                .expect("fixture parses");
        assert_eq!(golden.len(), 8);
        for g in golden {
            assert_eq!(get(&g.id).as_ref(), Some(&g), "{} changed", g.id);
        }
    }

    #[test]
    fn everyday_generic_parts_are_available() {
        let mut want: Vec<String> = vec![];
        for kind in ["R", "C", "L"] {
            for size in ["0402", "0603", "0805", "1206", "1210"] {
                want.push(format!("{kind}_{size}"));
            }
        }
        for n in 2..=10 {
            want.push(format!("CONN_1x{n:02}_P2.54"));
        }
        want.extend(
            [
                "LED_0603",
                "LED_0805",
                "D_SOD-123",
                "D_SMA",
                "Q_NPN_SOT-23",
                "Q_PNP_SOT-23",
                "Q_NMOS_SOT-23",
                "Q_PMOS_SOT-23",
                "LDO_SOT-23-5",
                "XTAL_HC-49US",
                "XTAL_3225",
                "SW_Tactile_6x6_THT",
                "C_1210",
                "ESP32-C3-MINI-1",
                "LMR33630_HSOIC-8",
                "SHT40_DFN-4",
                "AO4406A_SO-8",
                "L_SRP1038A",
                "L_WE-PD_7345",
                "L_WE-LQS_5040",
                "CP_Elec_6.3x5.4",
                "CP_Elec_8x10.2",
                "TERM_1x02_P5.08",
            ]
            .map(String::from),
        );
        for id in want {
            assert!(get(&id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn multi_pad_pins_join_the_right_pads() {
        let sw = get("SW_Tactile_6x6_THT").unwrap();
        assert_eq!(sw.pin_map["1"], ["1", "2"]);
        assert_eq!(sw.pin_map["2"], ["3", "4"]);
        let x = get("XTAL_3225").unwrap();
        assert_eq!(x.pin_map["2"], ["2", "4"]);
        // Crystal terminals are the diagonal pads 1 and 3.
        let (p1, p3) = (x.pad("1").unwrap(), x.pad("3").unwrap());
        assert!(p1.position.x == -p3.position.x && p1.position.y == -p3.position.y);
    }

    #[test]
    fn board_parts_follow_their_datasheet_pinouts() {
        let esp = get("ESP32-C3-MINI-1").unwrap();
        assert_eq!(esp.footprint.pads.len(), 61); // 52 numbered pads + 9-pad GND array
        assert_eq!(esp.pin("3").unwrap().name, "3V3");
        assert_eq!(esp.pin("8").unwrap().name, "EN");
        assert_eq!(esp.pin("26").unwrap().name, "IO18/USB_D-");
        assert_eq!(esp.pin("27").unwrap().name, "IO19/USB_D+");
        assert_eq!(esp.pin_map["1"].len(), 30); // GND 1, 2, 11, 14, 36-48, 50-53, 49a-i
                                                // Pin 1 is the top pad of the left column.
        let p1 = esp.pad("1").unwrap();
        assert!(esp
            .footprint
            .pads
            .iter()
            .filter(|p| p.position.x == p1.position.x)
            .all(|p| p.position.y <= p1.position.y));

        let buck = get("LMR33630_HSOIC-8").unwrap();
        let names: Vec<&str> = (1..=9)
            .map(|n| buck.pin(&n.to_string()).unwrap().name.as_str())
            .collect();
        assert_eq!(
            names,
            ["PGND", "VIN", "EN", "PG", "FB", "VCC", "BOOT", "SW", "AGND"]
        );

        let sht = get("SHT40_DFN-4").unwrap();
        assert_eq!(
            sht.footprint.pads.len(),
            4,
            "no centre pad under the sensor"
        );

        let fet = get("AO4406A_SO-8").unwrap();
        assert_eq!(fet.pin_map["1"], ["1", "2", "3"]);
        assert_eq!(fet.pin_map["5"], ["5", "6", "7", "8"]);

        // The buck inductor must not saturate below the regulator's current limit.
        let limit = buck
            .ratings
            .iter()
            .find(|r| r.name == "high_side_current_limit_max")
            .unwrap()
            .value;
        let isat = get("L_SRP1038A")
            .unwrap()
            .ratings
            .into_iter()
            .find(|r| r.name == "saturation_current")
            .unwrap()
            .value;
        assert!(isat > limit);
    }
}
