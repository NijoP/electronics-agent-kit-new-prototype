//! Project-specific component definitions, submitted to EAK with the typed
//! `UpsertComponentDefinition` command (the same path the symbol/footprint editor uses).
//!
//! Provenance rules: a rating is `verified: true` only when its value was read from the cited
//! datasheet during this project; geometry that had to be interpreted from a drawing is
//! called out in `provenance` and listed as NOT VERIFIED in READINESS.md.

use eak_core::geom::{mm, Point};
use eak_core::library::*;
use std::collections::BTreeMap;

pub const ESP32_DS: &str = "https://documentation.espressif.com/esp32-c3-wroom-02_datasheet_en.pdf";
pub const USB4105_DS: &str = "https://gct.co/files/drawings/usb4105.pdf";

pub const ESP32_ID: &str = "ESP32-C3-WROOM-02";
pub const USBC_ID: &str = "USB4105-GF-A";
pub const TACT_ID: &str = "SW_TACT_6x6_THT";
pub const MH_ID: &str = "MountingHole_3.2mm_M3_Pad";

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

fn tht(number: &str, x: f64, y: f64, sx: f64, sy: f64, drill: f64, shape: PadShape) -> PadDef {
    PadDef {
        number: number.into(),
        kind: PadKind::Tht,
        shape,
        position: Point::mm(x, y),
        size_x: mm(sx),
        size_y: mm(sy),
        drill: Some(mm(drill)),
    }
}

fn rating(name: &str, value: f64, unit: &str, source: &str, verified: bool) -> Rating {
    Rating {
        name: name.into(),
        value,
        unit: unit.into(),
        source: source.into(),
        verified,
    }
}

/// Espressif ESP32-C3-WROOM-02 (4 MB flash, PCB antenna).
///
/// * Pins: datasheet v1.7 Table 3-1 (19 pins; 9 and 19 are GND, 19 = EPAD).
/// * Land pattern: datasheet v1.7 Figure 11-1: 18 pads 1.5 x 0.9 mm on a 1.5 mm pitch, pad
///   rows 17.5 mm apart centre-to-centre, module 18 x 20 x 3.2 mm (Figure 10-1).
/// * Interpreted from the drawing (NOT VERIFIED): pin-1 row 6.4 mm below the antenna-side
///   edge, EPAD 2.9 x 2.9 mm modelled as one pad centred at (-0.85, +0.51) mm.
pub fn esp32_c3_wroom_02() -> ComponentDef {
    let io = PinType::Bidirectional;
    let left: [(&str, &str, PinType); 9] = [
        ("1", "3V3", PinType::PowerIn),
        ("2", "EN", PinType::Input),
        ("3", "IO4", io),
        ("4", "IO5", io),
        ("5", "IO6", io),
        ("6", "IO7", io),
        ("7", "IO8", io),
        ("8", "IO9", io),
        ("9", "GND", PinType::PowerIn),
    ];
    let right: [(&str, &str, PinType); 10] = [
        ("18", "IO0", io),
        ("17", "IO1", io),
        ("16", "IO2", io),
        ("15", "IO3", io),
        ("14", "IO19", io),
        ("13", "IO18", io),
        ("12", "TXD", io),
        ("11", "RXD", io),
        ("10", "IO10", io),
        ("19", "GND", PinType::PowerIn),
    ];
    let mut pins = vec![];
    for (i, (n, name, t)) in left.iter().enumerate() {
        pins.push(pin(n, name, *t, -12.7, 11.43 - 2.54 * i as f64));
    }
    for (i, (n, name, t)) in right.iter().enumerate() {
        pins.push(pin(n, name, *t, 12.7, 11.43 - 2.54 * i as f64));
    }
    let mut pads = vec![];
    for i in 0..9 {
        // Left column: pins 1..9 from the antenna side downwards.
        pads.push(smd(
            &(i + 1).to_string(),
            -8.75,
            -3.6 + 1.5 * i as f64,
            1.5,
            0.9,
        ));
        // Right column: pin 10 opposite pin 9, pin 18 opposite pin 1.
        pads.push(smd(
            &(10 + i).to_string(),
            8.75,
            8.4 - 1.5 * i as f64,
            1.5,
            0.9,
        ));
    }
    pads.push(smd("19", -0.85, 0.51, 2.9, 2.9));
    let pin_map: BTreeMap<String, Vec<String>> = (1..=19)
        .map(|i| (i.to_string(), vec![i.to_string()]))
        .collect();
    let src = format!("{ESP32_DS} (v1.7)");
    ComponentDef {
        id: ESP32_ID.into(),
        name: "ESP32-C3-WROOM-02 Wi-Fi/BLE module (RISC-V, 4 MB flash, PCB antenna)".into(),
        category: "MCU Module".into(),
        refdes_prefix: "U".into(),
        default_value: "ESP32-C3-WROOM-02-N4".into(),
        manufacturer: Some("Espressif Systems".into()),
        mpn: Some("ESP32-C3-WROOM-02-N4".into()),
        lcsc: None,
        datasheet: Some(ESP32_DS.into()),
        symbol: SymbolDef {
            pins,
            body_half_w: mm(10.16),
            body_half_h: mm(13.97),
        },
        footprint: FootprintDef {
            name: "ESP32-C3-WROOM-02".into(),
            pads,
            courtyard_half_w: mm(9.75),
            courtyard_half_h: mm(10.25),
            body_height: Some(mm(3.2)),
        },
        pin_map,
        ratings: vec![
            rating("supply_voltage_min", 3.0, "V", &format!("{src} Table 6-2"), true),
            rating("supply_voltage_max", 3.6, "V", &format!("{src} Table 6-2"), true),
            rating("supply_voltage_abs_max", 3.6, "V", &format!("{src} Table 6-1"), true),
            rating("supply_current_capability_min", 0.5, "A", &format!("{src} Table 6-2 (IVDD)"), true),
            rating("peak_tx_current", 0.345, "A", &format!("{src} Table 6-4 (802.11b 1 Mbps @20.5 dBm)"), true),
            rating("operating_temp_max", 85.0, "C", &format!("{src} Table 6-2 (85 C version)"), true),
        ],
        model3d: Model3d {
            kind: "box".into(),
            color: "#c0c0c0".into(),
            external_ref: None,
        },
        provenance: format!("Pin table and pad size/pitch/row spacing from {src} Table 3-1, Figures 10-1 and 11-1. Pin-1 row offset and EPAD position were interpreted from the drawing text and are NOT VERIFIED; EPAD modelled as a single 2.9 mm pad (drawing shows a 3x3 array)."),
    }
}

/// GCT USB4105-GF-A USB 2.0 Type-C receptacle (SMT, top mount).
///
/// Pad order and widths from the GCT drawing "Recommended PCB Layout" (rev B4): 16 contacts in
/// the order A1 B12 A4 B9 B8 A5 B7 A6 A7 B6 A8 B5 B4 A9 B1 A12, merged into 12 lands
/// (4 x 0.60 mm power lands, 8 x 0.30 mm signal lands, 0.50 mm signal pitch, 1.15 mm long).
/// Shell tabs are slots in the drawing (0.6 x 1.7 / 0.6 x 1.4 mm); EAK pads only support
/// round drills, so they are modelled as 0.6 mm round holes in oval lands — NOT
/// manufacturable as-is (see READINESS.md gap G2).
pub fn usb4105() -> ComponentDef {
    let pins = vec![
        pin("1", "VBUS", PinType::Passive, -10.16, 7.62),
        pin("2", "GND", PinType::Passive, -10.16, 5.08),
        pin("3", "CC1", PinType::Passive, -10.16, 2.54),
        pin("4", "CC2", PinType::Passive, -10.16, 0.0),
        pin("5", "D+", PinType::Passive, -10.16, -2.54),
        pin("6", "D-", PinType::Passive, -10.16, -5.08),
        pin("7", "SBU1", PinType::Passive, -10.16, -7.62),
        pin("8", "SBU2", PinType::Passive, -10.16, -10.16),
        pin("9", "SHIELD", PinType::Passive, -10.16, -12.7),
    ];
    // Origin = body centre; land row at y = -3.4, board edge at y = +3.2.
    let row = -3.4;
    let lands = [
        ("A1B12", -3.20, 0.6),
        ("A4B9", -2.40, 0.6),
        ("B8", -1.75, 0.3),
        ("A5", -1.25, 0.3),
        ("B7", -0.75, 0.3),
        ("A6", -0.25, 0.3),
        ("A7", 0.25, 0.3),
        ("B6", 0.75, 0.3),
        ("A8", 1.25, 0.3),
        ("B5", 1.75, 0.3),
        ("B4A9", 2.40, 0.6),
        ("B1A12", 3.20, 0.6),
    ];
    let mut pads: Vec<PadDef> = lands
        .iter()
        .map(|(n, x, w)| smd(n, *x, row, *w, 1.15))
        .collect();
    pads.push(tht("S1", -4.32, row + 0.915, 1.0, 2.1, 0.6, PadShape::Oval));
    pads.push(tht("S2", 4.32, row + 0.915, 1.0, 2.1, 0.6, PadShape::Oval));
    pads.push(tht("S3", -4.32, row + 5.095, 1.0, 1.8, 0.6, PadShape::Oval));
    pads.push(tht("S4", 4.32, row + 5.095, 1.0, 1.8, 0.6, PadShape::Oval));
    let mut pin_map = BTreeMap::new();
    let mut m = |p: &str, pads: &[&str]| {
        pin_map.insert(p.to_string(), pads.iter().map(|s| s.to_string()).collect());
    };
    m("1", &["A4B9", "B4A9"]);
    m("2", &["A1B12", "B1A12"]);
    m("3", &["A5"]);
    m("4", &["B5"]);
    m("5", &["A6", "B6"]);
    m("6", &["A7", "B7"]);
    m("7", &["A8"]);
    m("8", &["B8"]);
    m("9", &["S1", "S2", "S3", "S4"]);
    ComponentDef {
        id: USBC_ID.into(),
        name: "USB Type-C receptacle, USB 2.0, SMT top mount".into(),
        category: "Connector".into(),
        refdes_prefix: "J".into(),
        default_value: "USB4105-GF-A".into(),
        manufacturer: Some("GCT".into()),
        mpn: Some("USB4105-GF-A".into()),
        lcsc: None,
        datasheet: Some(USB4105_DS.into()),
        symbol: SymbolDef {
            pins,
            body_half_w: mm(5.08),
            body_half_h: mm(12.7),
        },
        footprint: FootprintDef {
            name: "USB_C_Receptacle_GCT_USB4105".into(),
            pads,
            courtyard_half_w: mm(5.0),
            courtyard_half_h: mm(4.2),
            body_height: Some(mm(3.31)),
        },
        pin_map,
        ratings: vec![
            rating("current_per_vbus_contact", 3.0, "A", "unverified (USB Type-C spec value, not read from the GCT drawing)", false),
        ],
        model3d: Model3d { kind: "box".into(), color: "#d0d0d0".into(), external_ref: None },
        provenance: format!("Land order, widths, pitch and land length from {USB4105_DS} (Recommended PCB Layout). Shell-tab positions interpreted from the drawing (NOT VERIFIED); slots approximated by round drills (EAK has no slotted drill)."),
    }
}

/// 6 x 6 mm through-hole tactile switch (4 legs; legs 1-2 and 3-4 are common).
pub fn tact_switch() -> ComponentDef {
    let pins = vec![
        pin("1", "A", PinType::Passive, -5.08, 0.0),
        pin("2", "B", PinType::Passive, 5.08, 0.0),
    ];
    let pads = vec![
        tht("1", -3.25, -2.25, 1.7, 1.7, 1.0, PadShape::Circle),
        tht("2", 3.25, -2.25, 1.7, 1.7, 1.0, PadShape::Circle),
        tht("3", -3.25, 2.25, 1.7, 1.7, 1.0, PadShape::Circle),
        tht("4", 3.25, 2.25, 1.7, 1.7, 1.0, PadShape::Circle),
    ];
    let mut pin_map = BTreeMap::new();
    pin_map.insert("1".into(), vec!["1".into(), "2".into()]);
    pin_map.insert("2".into(), vec!["3".into(), "4".into()]);
    ComponentDef {
        id: TACT_ID.into(),
        name: "Tactile switch 6x6 mm THT".into(),
        category: "Switch".into(),
        refdes_prefix: "SW".into(),
        default_value: "TACT_6x6".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        symbol: SymbolDef { pins, body_half_w: mm(2.54), body_half_h: mm(1.27) },
        footprint: FootprintDef {
            name: "SW_PUSH_6mm_THT".into(),
            pads,
            courtyard_half_w: mm(4.4),
            courtyard_half_h: mm(3.5),
            body_height: Some(mm(5.0)),
        },
        pin_map,
        ratings: vec![rating("contact_rating_current", 0.05, "A", "unverified (generic 6x6 tact switch class value)", false)],
        model3d: Model3d { kind: "box".into(), color: "#303030".into(), external_ref: None },
        provenance: "Generic 6x6 mm tact switch: 6.5 x 4.5 mm leg grid, 1.0 mm holes (common industry pattern, not tied to one manufacturer drawing; NOT VERIFIED)".into(),
    }
}

/// M3 mounting hole with a plated pad (connected to GND).
pub fn mounting_hole() -> ComponentDef {
    let pins = vec![pin("1", "MH", PinType::Passive, 0.0, -2.54)];
    ComponentDef {
        id: MH_ID.into(),
        name: "Mounting hole M3, plated, 6 mm pad".into(),
        category: "Mechanical".into(),
        refdes_prefix: "H".into(),
        default_value: "M3".into(),
        manufacturer: None,
        mpn: None,
        lcsc: None,
        datasheet: None,
        symbol: SymbolDef { pins: pins.clone(), body_half_w: mm(1.27), body_half_h: mm(1.27) },
        footprint: FootprintDef {
            name: "MountingHole_3.2mm_M3_Pad".into(),
            pads: vec![tht("1", 0.0, 0.0, 6.0, 6.0, 3.2, PadShape::Circle)],
            courtyard_half_w: mm(3.25),
            courtyard_half_h: mm(3.25),
            body_height: Some(mm(0.1)),
        },
        pin_map: [("1".to_string(), vec!["1".to_string()])].into_iter().collect(),
        ratings: vec![],
        model3d: Model3d { kind: "box".into(), color: "#b08a50".into(), external_ref: None },
        provenance: "ISO 273 medium clearance hole for M3 (3.2 mm) with 6 mm plated land; mechanical part, not placed by assembly".into(),
    }
}

pub fn all() -> Vec<ComponentDef> {
    vec![esp32_c3_wroom_02(), usb4105(), tact_switch(), mounting_hole()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn definitions_pass_eak_library_validation() {
        for d in super::all() {
            assert!(d.validate().is_empty(), "{}: {:?}", d.id, d.validate());
        }
    }
}
