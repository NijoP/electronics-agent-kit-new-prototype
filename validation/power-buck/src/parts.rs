//! Component definitions for the power-buck validation board.
//!
//! These are submitted to EAK through `Command::UpsertComponentDefinition`, which is EAK's
//! library-authoring path (symbol + footprint editor output). They are not written to the
//! project file directly.
//!
//! Rating provenance rules used here:
//! * `verified: true` only for values that were read from the manufacturer's datasheet
//!   during this validation session, with the section cited in `source`.
//! * Everything else is `verified: false`. `source` then names the document that must be
//!   checked, and the value is the designer's working assumption. It is NOT a datasheet fact.
//! * Land patterns are generic IPC-7351-style approximations, not the manufacturers'
//!   recommended land patterns. Each `provenance` string says so.

use eak_core::geom::{mm, Point};
use eak_core::library::*;
use std::collections::BTreeMap;

pub const TI_TPS54302: &str = "TI TPS54302 datasheet SLVSDG6C (Rev. C), https://www.ti.com/lit/ds/symlink/tps54302.pdf (values read from the TI HTML datasheet viewer https://www.ti.com/document-viewer/TPS54302/datasheet)";
const GENERIC_LAND: &str = "land pattern: generic IPC-7351-style approximation authored for this validation board; NOT checked against the manufacturer's recommended land pattern";

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

fn verified(name: &str, value: f64, unit: &str, section: &str) -> Rating {
    Rating {
        name: name.into(),
        value,
        unit: unit.into(),
        source: format!("{TI_TPS54302} {section}"),
        verified: true,
    }
}

fn assumed(name: &str, value: f64, unit: &str, check_against: &str) -> Rating {
    Rating {
        name: name.into(),
        value,
        unit: unit.into(),
        source: format!("UNVERIFIED designer assumption; must be checked against {check_against}"),
        verified: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn base(
    id: &str,
    name: &str,
    category: &str,
    prefix: &str,
    value: &str,
    mfr: &str,
    mpn: &str,
    datasheet: &str,
    pins: Vec<SymbolPin>,
    pin_map: BTreeMap<String, Vec<String>>,
    fp_name: &str,
    pads: Vec<PadDef>,
    court: (f64, f64),
    height: f64,
    ratings: Vec<Rating>,
    model: (&str, &str),
    provenance: &str,
) -> ComponentDef {
    ComponentDef {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        refdes_prefix: prefix.into(),
        default_value: value.into(),
        manufacturer: Some(mfr.into()),
        mpn: Some(mpn.into()),
        lcsc: None,
        datasheet: Some(datasheet.into()),
        pin_map,
        symbol: SymbolDef {
            pins,
            body_half_w: mm(5.08),
            body_half_h: mm(5.08),
        },
        footprint: FootprintDef {
            name: fp_name.into(),
            pads,
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
        provenance: format!("{provenance}; {GENERIC_LAND}"),
    }
}

/// Two-pin passive with pins 1/2 at x = ∓pitch_half.
#[allow(clippy::too_many_arguments)]
fn two_pin(
    id: &str,
    name: &str,
    category: &str,
    prefix: &str,
    value: &str,
    mfr: &str,
    mpn: &str,
    datasheet: &str,
    pin_names: (&str, &str),
    pad: (f64, f64),
    pitch_half: f64,
    court: (f64, f64),
    height: f64,
    ratings: Vec<Rating>,
) -> ComponentDef {
    let pins = vec![
        pin("1", pin_names.0, PinType::Passive, -5.08, 0.0),
        pin("2", pin_names.1, PinType::Passive, 5.08, 0.0),
    ];
    let map = one_to_one(&pins);
    base(
        id,
        name,
        category,
        prefix,
        value,
        mfr,
        mpn,
        datasheet,
        pins,
        map,
        id,
        vec![
            smd("1", -pitch_half, 0.0, pad.0, pad.1),
            smd("2", pitch_half, 0.0, pad.0, pad.1),
        ],
        court,
        height,
        ratings,
        ("box", "#806040"),
        "Electrical data per the cited datasheet where marked verified",
    )
}

/// TI TPS54302 synchronous buck converter, SOT-23-6 (DDC).
/// Pinout per SLVSDG6C Table 4-1: 1 GND, 2 SW, 3 VIN, 4 FB, 5 EN, 6 BOOT.
pub fn tps54302() -> ComponentDef {
    let pins = vec![
        pin("1", "GND", PinType::PowerIn, -10.16, 2.54),
        pin("2", "SW", PinType::PowerOut, -10.16, 0.0),
        pin("3", "VIN", PinType::PowerIn, -10.16, -2.54),
        pin("4", "FB", PinType::Input, 10.16, -2.54),
        pin("5", "EN", PinType::Input, 10.16, 0.0),
        pin("6", "BOOT", PinType::Passive, 10.16, 2.54),
    ];
    let map = one_to_one(&pins);
    // Pin 1 top-left, counter-clockwise (Y up), 0.95 mm pitch.
    let pads = vec![
        smd("1", -1.14, 0.95, 1.1, 0.6),
        smd("2", -1.14, 0.0, 1.1, 0.6),
        smd("3", -1.14, -0.95, 1.1, 0.6),
        smd("4", 1.14, -0.95, 1.1, 0.6),
        smd("5", 1.14, 0.0, 1.1, 0.6),
        smd("6", 1.14, 0.95, 1.1, 0.6),
    ];
    let s51 = "§5.1 Absolute Maximum Ratings";
    let s54 = "§5.4 Thermal Information (DDC)";
    let s55 = "§5.5 Electrical Characteristics";
    base(
        "TPS54302DDCR",
        "TPS54302 4.5-28 V 3 A synchronous buck converter (SOT-23-6 DDC)",
        "Power",
        "U",
        "TPS54302DDCR",
        "Texas Instruments",
        "TPS54302DDCR",
        "https://www.ti.com/lit/ds/symlink/tps54302.pdf",
        pins,
        map,
        "SOT-23-6_DDC",
        pads,
        (1.9, 1.7),
        1.1,
        vec![
            verified("vin_operating_min", 4.5, "V", s55),
            verified("vin_operating_max", 28.0, "V", s55),
            verified("vin_abs_max", 30.0, "V", s51),
            verified("sw_abs_max", 30.0, "V", s51),
            verified("en_abs_max", 7.0, "V", s51),
            verified("boot_sw_abs_max", 7.0, "V", s51),
            verified("vfb_min", 0.581, "V", s55),
            verified("vfb_typ", 0.596, "V", s55),
            verified("vfb_max", 0.611, "V", s55),
            verified("fsw_min", 290e3, "Hz", s55),
            verified("fsw_typ", 400e3, "Hz", s55),
            verified("fsw_max", 510e3, "Hz", s55),
            verified("rdson_high_side_typ", 0.085, "ohm", s55),
            verified("rdson_low_side_typ", 0.040, "ohm", s55),
            verified("ilim_high_side_min", 4.0, "A", s55),
            verified("ilim_high_side_max", 6.0, "A", s55),
            verified("ilim_low_side_min", 3.1, "A", s55),
            verified("ilim_low_side_max", 5.5, "A", s55),
            verified("theta_ja_jedec", 118.9, "C/W", s54),
            verified("theta_ja_evm", 57.2, "C/W", s54),
            verified("psi_jt", 14.0, "C/W", s54),
            verified("tj_abs_max", 150.0, "C", s51),
            verified("thermal_shutdown_rising", 160.0, "C", s55),
        ],
        ("box", "#202020"),
        "Pinout and ratings from TI SLVSDG6C Table 4-1, §5.1, §5.4, §5.5",
    )
}

/// AOS AO4407A P-channel MOSFET, SOIC-8: pins 1-3 source, 4 gate, 5-8 drain.
pub fn ao4407a() -> ComponentDef {
    let pins = vec![
        pin("1", "S", PinType::Passive, 7.62, 0.0),
        pin("2", "G", PinType::Passive, 0.0, -7.62),
        pin("3", "D", PinType::Passive, -7.62, 0.0),
    ];
    let mut map = BTreeMap::new();
    map.insert("1".into(), vec!["1".into(), "2".into(), "3".into()]);
    map.insert("2".into(), vec!["4".into()]);
    map.insert(
        "3".into(),
        vec!["5".into(), "6".into(), "7".into(), "8".into()],
    );
    let pads: Vec<PadDef> = (1..=8)
        .map(|i| {
            let (x, y) = if i <= 4 {
                (-2.7, 1.905 - 1.27 * (i - 1) as f64)
            } else {
                (2.7, -1.905 + 1.27 * (i - 5) as f64)
            };
            smd(&i.to_string(), x, y, 1.55, 0.6)
        })
        .collect();
    let ds = "AOS AO4407A datasheet https://www.aosmd.com/res/data_sheets/AO4407A.pdf";
    base(
        "AO4407A",
        "AO4407A -30 V P-channel MOSFET (SOIC-8), reverse-polarity switch",
        "Transistor",
        "Q",
        "AO4407A",
        "Alpha & Omega Semiconductor",
        "AO4407A",
        "https://www.aosmd.com/res/data_sheets/AO4407A.pdf",
        pins,
        map,
        "SOIC-8_3.9x4.9mm_P1.27mm",
        pads,
        (3.7, 2.7),
        1.75,
        vec![
            assumed("vds_max", 30.0, "V", ds),
            assumed("vgs_max", 25.0, "V", ds),
            assumed("id_continuous_25c", 12.0, "A", ds),
            assumed("rdson_vgs_10v_max", 0.014, "ohm", ds),
        ],
        ("box", "#202020"),
        "Pinout follows the common SOIC-8 P-MOSFET arrangement (S=1-3, G=4, D=5-8); must be checked against the AO4407A datasheet",
    )
}

pub fn terminal_2p() -> ComponentDef {
    let pins = vec![
        pin("1", "P1", PinType::Passive, -5.08, 1.27),
        pin("2", "P2", PinType::Passive, -5.08, -1.27),
    ];
    let map = one_to_one(&pins);
    base(
        "TERM_1x02_P5.08",
        "2-pole PCB screw terminal, 5.08 mm pitch",
        "Connector",
        "J",
        "MKDS 1,5/2-5,08",
        "Phoenix Contact",
        "1715721",
        "https://www.phoenixcontact.com/en-us/products/pcb-terminal-block-mkds-15-2-508-1715721",
        pins,
        map,
        "TerminalBlock_1x02_P5.08mm",
        vec![
            tht("1", -2.54, 0.0, 2.6, 1.3, true),
            tht("2", 2.54, 0.0, 2.6, 1.3, false),
        ],
        (5.3, 4.6),
        10.0,
        vec![assumed(
            "current",
            13.5,
            "A",
            "Phoenix Contact 1715721 product data",
        )],
        ("box", "#30a030"),
        "Generic 5.08 mm two-pole terminal block",
    )
}

pub fn mounting_hole() -> ComponentDef {
    let pins = vec![pin("1", "MH", PinType::NoConnect, 0.0, 0.0)];
    let map = one_to_one(&pins);
    base(
        "MH_M3_PTH",
        "M3 mounting hole, plated, 6 mm pad, unconnected",
        "Mechanical",
        "H",
        "M3",
        "n/a",
        "n/a",
        "n/a",
        pins,
        map,
        "MountingHole_3.2mm_M3_Pad",
        vec![tht("1", 0.0, 0.0, 6.0, 3.2, false)],
        (3.2, 3.2),
        0.1,
        vec![],
        ("cylinder", "#c0c0c0"),
        "Mounting hole: 3.2 mm drill for M3, 6.0 mm annular pad, left floating (no net)",
    )
}

pub fn fuse_1206() -> ComponentDef {
    let ds = "Bourns SF-1206S series datasheet https://www.bourns.com/docs/product-datasheets/sf-1206s.pdf";
    two_pin(
        "FUSE_1206_3A_SLOW",
        "3 A slow-blow SMD fuse, 1206",
        "Fuse",
        "F",
        "3A slow",
        "Bourns",
        "SF-1206S300-2",
        "https://www.bourns.com/docs/product-datasheets/sf-1206s.pdf",
        ("~", "~"),
        (1.15, 1.8),
        1.45,
        (2.3, 1.25),
        0.9,
        vec![
            assumed("current", 3.0, "A", ds),
            assumed("voltage", 32.0, "V", ds),
        ],
    )
}

pub fn zener_12v() -> ComponentDef {
    let ds = "Diodes Inc. MMSZ5221B-MMSZ5259B datasheet https://www.diodes.com/assets/Datasheets/ds18010.pdf";
    two_pin(
        "MMSZ5242B_SOD-123",
        "12 V 500 mW Zener diode, SOD-123 (gate clamp)",
        "Diode",
        "D",
        "MMSZ5242B",
        "Diodes Incorporated",
        "MMSZ5242B-7-F",
        "https://www.diodes.com/assets/Datasheets/ds18010.pdf",
        ("K", "A"),
        (0.9, 1.2),
        1.65,
        (2.3, 1.0),
        1.35,
        vec![
            assumed("zener_voltage", 12.0, "V", ds),
            assumed("power", 0.5, "W", ds),
        ],
    )
}

pub fn tvs_smaj26a() -> ComponentDef {
    let ds = "Vishay SMAJ series datasheet https://www.vishay.com/docs/88390/smaj50a.pdf";
    two_pin(
        "SMAJ26A_SMA",
        "SMAJ26A 400 W unidirectional TVS, SMA (DO-214AC)",
        "Diode",
        "D",
        "SMAJ26A",
        "Vishay",
        "SMAJ26A-E3/61",
        "https://www.vishay.com/docs/88390/smaj50a.pdf",
        ("K", "A"),
        (2.5, 1.7),
        2.0,
        (3.3, 1.6),
        2.4,
        vec![
            assumed("vrwm", 26.0, "V", ds),
            assumed("vbr_min", 28.9, "V", ds),
            assumed("vc_at_ipp", 42.1, "V", ds),
            assumed("ppp", 400.0, "W", ds),
        ],
    )
}

pub fn cap_elec_47u_50v() -> ComponentDef {
    let ds = "Panasonic EEE-FK aluminium electrolytic datasheet (industrial.panasonic.com)";
    two_pin(
        "CP_EEE-FK1H470P",
        "47 µF 50 V aluminium electrolytic, SMD 8x10.2 mm (input bulk / damping)",
        "Capacitor",
        "C",
        "47u 50V",
        "Panasonic",
        "EEE-FK1H470P",
        "https://industrial.panasonic.com/",
        ("+", "-"),
        (3.5, 2.5),
        3.35,
        (4.6, 4.6),
        10.5,
        vec![assumed("voltage", 50.0, "V", ds)],
    )
}

pub fn cap_1210_10u_50v() -> ComponentDef {
    let ds = "Murata GRM32ER71H106KA12 product data (psearch.en.murata.com)";
    two_pin(
        "C_1210_10u_50V_X7R",
        "10 µF 50 V X7R MLCC 1210 (input)",
        "Capacitor",
        "C",
        "10u 50V X7R",
        "Murata",
        "GRM32ER71H106KA12L",
        "https://www.murata.com/en-us/products/productdetail?partno=GRM32ER71H106KA12%23",
        ("~", "~"),
        (1.2, 2.7),
        1.5,
        (2.3, 1.6),
        2.7,
        vec![assumed("voltage", 50.0, "V", ds)],
    )
}

pub fn cap_1210_22u_25v() -> ComponentDef {
    let ds = "Murata GRM32ER71E226KE15 product data (psearch.en.murata.com)";
    two_pin(
        "C_1210_22u_25V_X7R",
        "22 µF 25 V X7R MLCC 1210 (output)",
        "Capacitor",
        "C",
        "22u 25V X7R",
        "Murata",
        "GRM32ER71E226KE15L",
        "https://www.murata.com/en-us/products/productdetail?partno=GRM32ER71E226KE15%23",
        ("~", "~"),
        (1.2, 2.7),
        1.5,
        (2.3, 1.6),
        2.7,
        vec![assumed("voltage", 25.0, "V", ds)],
    )
}

pub fn cap_0603_100n_50v() -> ComponentDef {
    let ds = "Murata GRM188R71H104KA93 product data (psearch.en.murata.com)";
    two_pin(
        "C_0603_100n_50V_X7R",
        "100 nF 50 V X7R MLCC 0603",
        "Capacitor",
        "C",
        "100n 50V X7R",
        "Murata",
        "GRM188R71H104KA93D",
        "https://www.murata.com/en-us/products/productdetail?partno=GRM188R71H104KA93%23",
        ("~", "~"),
        (0.8, 0.95),
        0.825,
        (1.5, 0.75),
        0.9,
        vec![assumed("voltage", 50.0, "V", ds)],
    )
}

pub fn inductor_10u() -> ComponentDef {
    let ds =
        "Bourns SRP1265A datasheet https://www.bourns.com/docs/product-datasheets/srp1265a.pdf";
    two_pin(
        "SRP1265A-100M",
        "10 µH shielded power inductor, 13.5x12.5x6.5 mm",
        "Inductor",
        "L",
        "10u",
        "Bourns",
        "SRP1265A-100M",
        "https://www.bourns.com/docs/product-datasheets/srp1265a.pdf",
        ("~", "~"),
        (3.6, 4.0),
        5.0,
        (7.0, 6.6),
        6.7,
        vec![
            assumed("inductance", 10e-6, "H", ds),
            assumed("tolerance", 0.2, "ratio", ds),
            assumed("isat", 12.0, "A", ds),
            assumed("irms", 7.5, "A", ds),
            assumed("dcr_max", 0.020, "ohm", ds),
        ],
    )
}

/// All custom definitions used by the board.
pub fn all() -> Vec<ComponentDef> {
    vec![
        tps54302(),
        ao4407a(),
        terminal_2p(),
        mounting_hole(),
        fuse_1206(),
        zener_12v(),
        tvs_smaj26a(),
        cap_elec_47u_50v(),
        cap_1210_10u_50v(),
        cap_1210_22u_25v(),
        cap_0603_100n_50v(),
        inductor_10u(),
    ]
}

/// Look up a rating value in a definition (used by the calculation report).
pub fn rating(def: &ComponentDef, name: &str) -> Option<f64> {
    def.ratings.iter().find(|r| r.name == name).map(|r| r.value)
}

#[cfg(test)]
mod tests {
    #[test]
    fn definitions_are_structurally_valid() {
        for d in super::all() {
            assert!(d.validate().is_empty(), "{}: {:?}", d.id, d.validate());
        }
    }
}
