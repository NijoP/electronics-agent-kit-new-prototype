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
        },
        provenance: GENERIC.into(),
    }
}

pub fn components() -> Vec<ComponentDef> {
    let mut v = vec![
        two_pin(
            "R_0603",
            "Resistor 0603",
            "Resistor",
            "R",
            "10k",
            ("~", "~"),
            0.8,
            0.95,
            0.825,
            (1.5, 0.75),
            0.45,
            ("box", "#202020"),
            vec![unverified("power", 0.1, "W")],
        ),
        two_pin(
            "C_0603",
            "Capacitor 0603 (MLCC)",
            "Capacitor",
            "C",
            "100n",
            ("~", "~"),
            0.8,
            0.95,
            0.825,
            (1.5, 0.75),
            0.8,
            ("box", "#b08a50"),
            vec![unverified("voltage", 16.0, "V")],
        ),
        two_pin(
            "C_0805",
            "Capacitor 0805 (MLCC)",
            "Capacitor",
            "C",
            "10u",
            ("~", "~"),
            1.0,
            1.45,
            0.95,
            (1.7, 1.0),
            1.25,
            ("box", "#b08a50"),
            vec![unverified("voltage", 16.0, "V")],
        ),
    ];

    // LED 0603: pin 1 = cathode, pin 2 = anode.
    let mut led = two_pin(
        "LED_0603",
        "LED 0603",
        "LED",
        "D",
        "Green",
        ("K", "A"),
        0.8,
        0.95,
        0.825,
        (1.5, 0.75),
        0.6,
        ("box", "#40c040"),
        vec![unverified("forward_current_max", 0.02, "A")],
    );
    led.symbol.pins[0].electrical_type = PinType::Passive;
    v.push(led);

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
        },
        provenance: GENERIC.into(),
    });

    // 4-pin 2.54 mm header (I/O).
    let hdr4_pins: Vec<SymbolPin> = (1..=4)
        .map(|i| {
            pin(
                &i.to_string(),
                &format!("P{i}"),
                PinType::Passive,
                -5.08,
                3.81 - 2.54 * (i - 1) as f64,
            )
        })
        .collect();
    v.push(ComponentDef {
        id: "CONN_1x04_P2.54".into(),
        name: "Pin header 1x04 2.54 mm".into(),
        category: "Connector".into(),
        refdes_prefix: "J".into(),
        default_value: "CONN_1x04".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        pin_map: one_to_one(&hdr4_pins),
        symbol: SymbolDef {
            pins: hdr4_pins,
            body_half_w: mm(2.54),
            body_half_h: mm(5.08),
        },
        footprint: FootprintDef {
            name: "PinHeader_1x04_P2.54mm".into(),
            pads: (1..=4)
                .map(|i| {
                    tht(
                        &i.to_string(),
                        0.0,
                        3.81 - 2.54 * (i - 1) as f64,
                        1.7,
                        1.0,
                        i == 1,
                    )
                })
                .collect(),
            courtyard_half_w: mm(1.5),
            courtyard_half_h: mm(5.3),
            body_height: Some(mm(8.5)),
        },
        ratings: vec![],
        model3d: Model3d {
            kind: "header".into(),
            color: "#151515".into(),
            external_ref: None,
        },
        provenance: GENERIC.into(),
    });

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
        model3d: Model3d { kind: "box".into(), color: "#2a2a2a".into(), external_ref: None },
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
        model3d: Model3d { kind: "box".into(), color: "#1a1a1a".into(), external_ref: None },
        provenance: "Pinout from Microchip ATtiny25/45/85 datasheet (as commonly published); land pattern generic".into(),
    });
    v
}

pub fn get(id: &str) -> Option<ComponentDef> {
    components().into_iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn builtin_library_is_structurally_valid() {
        for c in super::components() {
            let problems = c.validate();
            assert!(problems.is_empty(), "{}: {problems:?}", c.id);
        }
    }
}
