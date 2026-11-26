//! Round trips over fixtures written for these tests (no KiCad library file is copied):
//! import → EAK definitions → export → re-import gives the same definitions, and a second
//! export is byte-identical to the first.

use eak_core::geom::Point;
use eak_core::kicad_import::{FootprintImport, SymbolImport};
use eak_core::library::{PadKind, PadShape, PinOrientation, PinType};
use eak_core::sexpr::Limits;
use eak_core::Graphic;
use eak_library::kicad::{
    export_footprint, export_symbol_lib, import_footprint, import_symbol_lib, same_symbol,
};

const SYM: &str = include_str!("fixtures/eak_demo.kicad_sym");
const SOIC: &str = include_str!("fixtures/EAK_SOIC-8_Demo.kicad_mod");
const HEADER: &str = include_str!("fixtures/EAK_Header_1x03_Demo.kicad_mod");

fn symbols(src: &str) -> Vec<SymbolImport> {
    let r = import_symbol_lib(src, Limits::default()).expect("symbol library parses");
    assert_eq!(r.report.rejected(), 0, "{:?}", r.report);
    r.symbols
}

fn footprint(src: &str) -> FootprintImport {
    let r = import_footprint(src, Limits::default()).expect("footprint parses");
    assert_eq!(r.report.rejected(), 0, "{:?}", r.report);
    r.footprint.expect("footprint imported")
}

#[test]
fn fixture_symbols_import_as_expected() {
    let s = symbols(SYM);
    assert_eq!(s.len(), 2);
    let op = &s[0];
    assert_eq!(op.name, "EAK_OPAMP_DEMO");
    assert_eq!(op.def.pins.len(), 8);
    assert_eq!(op.def.graphics.len(), 6);
    assert_eq!(op.footprint.as_deref(), Some("EAK_Demo:EAK_SOIC-8_Demo"));
    assert_eq!(op.datasheet, None);
    assert_eq!(op.refdes_prefix.as_deref(), Some("U"));
    let vplus = op.def.pins.iter().find(|p| p.number == "8").unwrap();
    assert_eq!(vplus.electrical_type, PinType::PowerIn);
    assert_eq!(vplus.orientation, PinOrientation::Down);
    assert_eq!(vplus.position, Point::mm(0.0, 7.62));
    assert!(matches!(op.def.graphics[0], Graphic::Polygon { fill: true, .. }));
    assert!(matches!(op.def.graphics[2], Graphic::Arc { .. }));
    assert!(matches!(op.def.graphics[3], Graphic::Line { .. }));

    let r = &s[1];
    assert_eq!(r.value.as_deref(), Some("10k \"demo\""));
    for p in &r.def.pins {
        assert!(!p.name_visible && !p.number_visible && p.name.is_empty());
    }
    assert_eq!(r.def.body_half_w, 1_016_000);
    assert_eq!(r.def.body_half_h, 2_540_000);
}

#[test]
fn symbol_library_round_trips() {
    let first = symbols(SYM);
    let out = export_symbol_lib(&first);
    assert!(out.losses.is_empty(), "{:?}", out.losses);
    let again = import_symbol_lib(&out.text, Limits::default()).unwrap();
    assert_eq!(again.report.rejected(), 0, "{:?}", again.report);
    assert!(
        again.report.unsupported.is_empty(),
        "the exporter writes only what the reader interprets: {:?}",
        again.report.unsupported
    );
    assert_eq!(again.symbols.len(), first.len());
    for (a, b) in first.iter().zip(&again.symbols) {
        assert!(same_symbol(a, b), "{} changed:\n{a:#?}\n{b:#?}", a.name);
    }
    // The fixture is fully supported, so the import reports agree too.
    let original = import_symbol_lib(SYM, Limits::default()).unwrap().report;
    assert_eq!(original.items, again.report.items);
    // A second export is a fixed point.
    assert_eq!(export_symbol_lib(&again.symbols).text, out.text);
}

#[test]
fn smd_footprint_round_trips() {
    let a = footprint(SOIC);
    assert_eq!(a.def.pads.len(), 8);
    assert!(a.def.pads.iter().all(|p| p.kind == PadKind::Smd));
    assert_eq!(a.def.courtyard_half_w, 3_700_000);
    assert_eq!(a.def.courtyard_half_h, 2_700_000);
    assert_eq!(
        a.model_path.as_deref(),
        Some("${EAK_DEMO_3D}/EAK_SOIC-8_Demo.step")
    );
    let out = export_footprint(&a);
    assert!(out.losses.is_empty(), "{:?}", out.losses);
    let again = import_footprint(&out.text, Limits::default()).unwrap();
    assert!(again.report.unsupported.is_empty(), "{:?}", again.report);
    let b = again.footprint.expect("re-imported");
    assert_eq!(a, b);
    assert_eq!(export_footprint(&b).text, out.text);
}

#[test]
fn through_hole_footprint_round_trips_with_rotation_and_drills_normalised() {
    let a = footprint(HEADER);
    assert_eq!(a.def.pads.len(), 3);
    let p2 = &a.def.pads[1];
    assert_eq!((p2.kind, p2.shape), (PadKind::Tht, PadShape::Oval));
    assert_eq!(p2.drill, Some(1_000_000), "slot imported by its smaller size");
    let p3 = &a.def.pads[2];
    assert_eq!((p3.size_x, p3.size_y), (2_000_000, 1_700_000), "90° turn applied");
    assert_eq!(a.def.courtyard_half_h, 6_880_000);
    let out = export_footprint(&a);
    assert!(out.losses.is_empty(), "{:?}", out.losses);
    let b = footprint(&out.text);
    assert_eq!(a, b);
}

#[test]
fn what_the_format_cannot_carry_is_reported_as_a_loss() {
    let mut fp = footprint(SOIC);
    fp.def.body_height = Some(1_750_000);
    fp.def.pads[0].mask_expansion = Some(50_000);
    let out = export_footprint(&fp);
    assert_eq!(out.losses.len(), 2, "{:?}", out.losses);
    assert!(out.losses.iter().any(|l| l.contains("body height")));
    assert!(out.losses.iter().any(|l| l.contains("pad 1")));

    let mut syms = symbols(SYM);
    syms[0].def.pins[0].name_visible = false;
    let out = export_symbol_lib(&syms[..1]);
    assert_eq!(out.losses.len(), 1, "{:?}", out.losses);
    assert!(out.losses[0].contains("pin name visibility"));
}

#[test]
fn builtin_library_pins_and_pads_survive_export() {
    let comps: Vec<_> = eak_core::builtin::components()
        .into_iter()
        .filter(|c| !c.symbol.pins.is_empty() && !c.footprint.pads.is_empty())
        .collect();
    assert!(!comps.is_empty());
    let syms: Vec<SymbolImport> = comps
        .iter()
        .map(|c| SymbolImport {
            name: c.id.clone(),
            item_id: c.id.clone(),
            def: c.symbol.clone(),
            footprint: None,
            datasheet: c.datasheet.clone(),
            refdes_prefix: Some(c.refdes_prefix.clone()),
            value: Some(c.default_value.clone()),
        })
        .collect();
    let back = symbols(&export_symbol_lib(&syms).text);
    assert_eq!(back.len(), syms.len());
    for (a, b) in syms.iter().zip(&back) {
        assert_eq!(a.name, b.name);
        let key = |s: &SymbolImport| {
            s.def
                .pins
                .iter()
                .map(|p| {
                    // The format reads a pin named "~" as unnamed (reported as a loss).
                    let name = if p.name == "~" { "" } else { p.name.as_str() };
                    let t = p.electrical_type;
                    (p.number.clone(), name.to_string(), t, p.position, p.length)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(key(a), key(b), "pins of {}", a.name);
    }
    for c in &comps {
        let fp = FootprintImport {
            item_id: c.footprint.name.clone(),
            def: c.footprint.clone(),
            description: None,
            model_path: None,
        };
        let b = footprint(&export_footprint(&fp).text);
        let mut expect = c.footprint.pads.clone();
        for p in &mut expect {
            p.mask_expansion = None;
            p.paste_expansion = None;
            if p.kind == PadKind::Smd {
                p.drill = None;
            }
        }
        assert_eq!(b.def.pads, expect, "pads of {}", c.id);
        assert_eq!(b.def.courtyard_half_w, c.footprint.courtyard_half_w);
        assert_eq!(b.def.courtyard_half_h, c.footprint.courtyard_half_h);
    }
}
