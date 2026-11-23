//! Schema v4 PCB objects in DRC (ADR-0042): keep-out and constraint regions (DRC-021/022),
//! silk text and dimensions (DRC-023), board cut-outs and arc edges (DRC-005/009).

use eak_core::geom::polygon_area2;
use eak_core::{
    CopperLayer, Cutout, DimensionUnit, DrawLayer, OutlineArc, PcbDimension, PcbText, Point,
    Project, Region, RegionObject, Rotation, Track,
};
use eak_runtime::{fixtures, Engine};
use eak_validation::{drc, Finding, Report, Severity};

fn reference() -> Project {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb(&mut e).expect("pcb");
    e.project().clone()
}

fn fired<'a>(r: &'a Report, rule: &'a str) -> impl Iterator<Item = &'a Finding> {
    r.findings.iter().filter(move |f| f.rule_id == rule)
}

/// A top-layer track of the reference board at least 3 mm long, and its midpoint.
fn long_top_track(p: &Project) -> (Track, Point) {
    let t = p
        .pcb
        .tracks
        .values()
        .find(|t| t.layer == CopperLayer::Top && t.a.dist(t.b) >= 3_000_000.0)
        .expect("reference board has a long top track")
        .clone();
    let m = Point::new((t.a.x + t.b.x) / 2, (t.a.y + t.b.y) / 2);
    (t, m)
}

/// Square of side `s` nm centred on `c`.
fn square(c: Point, s: i64) -> Vec<Point> {
    let h = s / 2;
    vec![
        Point::new(c.x - h, c.y - h),
        Point::new(c.x + h, c.y - h),
        Point::new(c.x + h, c.y + h),
        Point::new(c.x - h, c.y + h),
    ]
}

fn region(id: &str, outline: Vec<Point>, layers: Vec<CopperLayer>) -> Region {
    Region {
        id: id.into(),
        name: String::new(),
        outline,
        layers,
        forbid: Default::default(),
        min_track_width: None,
        max_track_width: None,
    }
}

#[test]
fn new_rules_are_declared_and_keepouts_are_no_longer_not_implemented() {
    let r = drc::run(&reference());
    for id in ["DRC-021", "DRC-022", "DRC-023"] {
        assert!(r.rules.iter().any(|x| x.id == id), "{id} not declared");
        assert_eq!(fired(&r, id).count(), 0, "{id} fired on the reference");
    }
    assert!(!r
        .not_implemented
        .iter()
        .any(|n| n.to_lowercase().contains("keepout")));
    assert!(r.limitations.iter().any(|l| l.contains("keep-out")));
}

#[test]
fn drc_detects_a_track_in_a_keepout() {
    // One reference build for every case: fixture ids are random per build.
    let base = reference();
    let mut p = base.clone();
    let (t, m) = long_top_track(&p);
    let mut keep = region("rgn-1", square(m, 1_000_000), vec![CopperLayer::Top]);
    keep.forbid.insert(RegionObject::Tracks);
    p.pcb.regions.insert(keep.id.clone(), keep.clone());
    let r = drc::run(&p);
    let hit: Vec<_> = fired(&r, "DRC-021")
        .filter(|f| f.objects.contains(&t.id))
        .collect();
    assert_eq!(hit.len(), 1, "{:?}", r.findings);
    assert_eq!(hit[0].severity, Severity::Error);
    assert!(hit[0].objects.contains(&keep.id));
    assert!(hit[0].message.contains("keeps out tracks"));

    // The same region on the other layer, or forbidding only vias, does not fire for it.
    for (layers, forbid) in [
        (vec![CopperLayer::Bottom], RegionObject::Tracks),
        (vec![CopperLayer::Top], RegionObject::Vias),
    ] {
        let mut q = base.clone();
        let mut other = region("rgn-1", square(m, 1_000_000), layers);
        other.forbid.insert(forbid);
        q.pcb.regions.insert(other.id.clone(), other);
        let r = drc::run(&q);
        assert!(
            !fired(&r, "DRC-021").any(|f| f.objects.contains(&t.id)),
            "{:?}",
            r.findings
        );
    }
}

#[test]
fn drc_detects_a_via_in_a_keepout() {
    let mut p = reference();
    let v = p
        .pcb
        .vias
        .values()
        .next()
        .expect("reference has a via")
        .clone();
    let mut keep = region("rgn-v", square(v.position, 2_000_000), vec![]);
    keep.forbid.insert(RegionObject::Vias);
    p.pcb.regions.insert(keep.id.clone(), keep);
    let r = drc::run(&p);
    assert!(
        fired(&r, "DRC-021").any(|f| f.objects.contains(&v.id)),
        "{:?}",
        r.findings
    );
}

#[test]
fn constraint_region_checks_track_width() {
    // One reference build for all three cases: fixture ids are random per build.
    let base = reference();
    let (t, m) = long_top_track(&base);
    let mut p = base.clone();
    let mut narrow = region("rgn-w", square(m, 1_000_000), vec![]);
    narrow.min_track_width = Some(t.width + 100_000);
    p.pcb.regions.insert(narrow.id.clone(), narrow);
    let r = drc::run(&p);
    let f = fired(&r, "DRC-022")
        .find(|f| f.objects.contains(&t.id))
        .expect("DRC-022 fired");
    assert_eq!(f.severity, Severity::Error);
    assert!(f.message.contains("narrower"));
    assert!(f.required.is_some());

    let mut p = base.clone();
    let mut fits = region("rgn-w", square(m, 1_000_000), vec![]);
    fits.min_track_width = Some(t.width);
    fits.max_track_width = Some(t.width);
    p.pcb.regions.insert(fits.id.clone(), fits);
    assert!(!fired(&drc::run(&p), "DRC-022").any(|f| f.objects.contains(&t.id)));

    let mut p = base.clone();
    let mut wide = region("rgn-w", square(m, 1_000_000), vec![]);
    wide.max_track_width = Some(t.width - 50_000);
    p.pcb.regions.insert(wide.id.clone(), wide);
    let r = drc::run(&p);
    assert!(fired(&r, "DRC-022").any(|f| f.objects.contains(&t.id) && f.message.contains("wider")));
}

#[test]
fn copper_inside_a_cutout_breaks_the_edge_rule() {
    let mut p = reference();
    let (t, m) = long_top_track(&p);
    let cut = Cutout {
        id: "cut-1".into(),
        outline: square(m, 1_000_000),
    };
    p.pcb.cutouts.insert(cut.id.clone(), cut);
    let r = drc::run(&p);
    assert!(
        fired(&r, "DRC-005").any(|f| f.objects.contains(&t.id)
            && f.objects.contains(&"cut-1".to_string())
            && f.severity == Severity::Error),
        "{:?}",
        r.findings
    );
}

#[test]
fn arc_outline_edges_are_checked_on_the_flattened_outline() {
    let mut p = reference();
    let n = p.pcb.board.outline.len();
    let before = fired(&drc::run(&p), "DRC-005").count();
    // A positive sweep bulges to the right of the edge, which is outward on a CCW outline:
    // the outline stays simple and no copper moves closer to the edge.
    let ccw = polygon_area2(&p.pcb.board.outline) > 0;
    p.pcb.board.outline_arcs = vec![OutlineArc {
        edge: 0,
        sweep_deg: if ccw { 90.0 } else { -90.0 },
    }];
    assert!(p.pcb.board.outline_path().len() > n);
    let r = drc::run(&p);
    assert_eq!(fired(&r, "DRC-009").count(), 0, "{:?}", r.findings);
    assert!(fired(&r, "DRC-005").count() <= before);
}

#[test]
fn silk_text_and_dimension_labels_must_be_legible_and_drawable() {
    let mut p = reference();
    let text = |id: &str, layer, s: &str, size| PcbText {
        id: id.into(),
        layer,
        position: Point::mm(20.0, 2.0),
        text: s.into(),
        size,
        stroke_width: 0,
        rotation: Rotation::R0,
    };
    for t in [
        text("txt-small", DrawLayer::TopSilk, "REV A", 500_000),
        text("txt-glyph", DrawLayer::BottomSilk, "Ω 5V", 1_000_000),
        text("txt-ok", DrawLayer::TopSilk, "REV A", 1_000_000),
        text("txt-doc", DrawLayer::Documentation, "§ tiny", 100_000),
    ] {
        p.pcb.texts.insert(t.id.clone(), t);
    }
    p.pcb.dimensions.insert(
        "dim-1".into(),
        PcbDimension {
            id: "dim-1".into(),
            layer: DrawLayer::TopSilk,
            a: Point::mm(0.0, 0.0),
            b: Point::mm(40.0, 0.0),
            offset: -2_000_000,
            unit: DimensionUnit::Mm,
            precision: 2,
            text_size: 600_000,
        },
    );
    let r = drc::run(&p);
    let ids = |key: &str| -> Vec<String> {
        let mut v: Vec<String> = fired(&r, "DRC-023")
            .filter(|f| f.message.contains(key))
            .flat_map(|f| f.objects.clone())
            .collect();
        v.sort();
        v
    };
    assert_eq!(ids("legibility"), vec!["dim-1", "txt-small"]);
    assert_eq!(ids("cannot draw"), vec!["txt-glyph"]);
    assert!(fired(&r, "DRC-023").all(|f| f.severity == Severity::Warning));
}
