//! Design rules check: geometry, clearance, connectivity, drill, edge, current capacity,
//! thermal-relief spokes, silkscreen over soldermask openings and schematic/PCB consistency.

use crate::report::{finding, rule, Finding, Report, Severity::*};
use eak_core::geom::{point_in_polygon, point_segment_distance, polygon_self_intersects, to_mm};
use eak_core::{CopperLayer, Nm, PadKind, Project, Rect, Side};
use eak_engineering::copper::{self, distance, Core, ItemKind};
use eak_engineering::ipc::{self, ClearanceCondition};
use eak_engineering::{netlist, silk};
use std::collections::{BTreeMap, BTreeSet};

fn mm(v: f64) -> String {
    format!("{:.3} mm", v / 1e6)
}

pub fn run(p: &Project) -> Report {
    let c = &p.constraints;
    let mut f: Vec<Finding> = Vec::new();
    let outline = &p.pcb.board.outline;
    let has_outline = outline.len() >= 3 && !polygon_self_intersects(outline);
    if !has_outline {
        f.push(finding(
            "DRC-009",
            Error,
            "board outline is missing or not a simple closed polygon",
            vec![],
            None,
        ));
    }

    // DRC-001 / DRC-008: track widths, including IPC-2221 current capacity.
    for t in p.pcb.tracks.values() {
        if t.width < c.min_track_width {
            f.push(finding(
                "DRC-001",
                Error,
                format!(
                    "track width {} below minimum {}",
                    mm(t.width as f64),
                    mm(c.min_track_width as f64)
                ),
                vec![t.id.clone()],
                Some(t.a),
            ));
        } else if t.width < c.class_for(&t.net).track_width {
            f.push(finding(
                "DRC-001",
                Warning,
                format!(
                    "track on {} narrower ({}) than its net class width",
                    t.net,
                    mm(t.width as f64)
                ),
                vec![t.id.clone()],
                Some(t.a),
            ));
        }
        if let Some(&amps) = c.net_currents_a.get(&t.net) {
            match ipc::trace_width_mm(
                amps,
                false,
                p.pcb.board.stackup.outer_copper_oz,
                c.temp_rise_c,
            ) {
                Ok(req) => {
                    if to_mm(t.width) + 1e-9 < req {
                        f.push(finding(
                            "DRC-008",
                            Error,
                            format!("track on {} is {:.3} mm but {:.2} A needs ≥ {:.3} mm (IPC-2221, ΔT {} °C, {} oz; table not verified against standard text)", t.net, to_mm(t.width), amps, req, c.temp_rise_c, p.pcb.board.stackup.outer_copper_oz),
                            vec![t.id.clone()],
                            Some(t.a),
                        ));
                    }
                }
                Err(e) => f.push(finding(
                    "DRC-008",
                    Error,
                    format!("current sizing for {} failed: {e}", t.net),
                    vec![t.id.clone()],
                    None,
                )),
            }
        }
    }

    // DRC-006: drills and annular rings.
    for v in p.pcb.vias.values() {
        if v.drill < c.min_drill {
            f.push(finding(
                "DRC-006",
                Error,
                format!(
                    "via drill {} below minimum {}",
                    mm(v.drill as f64),
                    mm(c.min_drill as f64)
                ),
                vec![v.id.clone()],
                Some(v.position),
            ));
        }
        let ring = (v.diameter - v.drill) / 2;
        if ring < c.min_annular_ring {
            f.push(finding(
                "DRC-006",
                Error,
                format!(
                    "via annular ring {} below minimum {}",
                    mm(ring as f64),
                    mm(c.min_annular_ring as f64)
                ),
                vec![v.id.clone()],
                Some(v.position),
            ));
        }
    }
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in def.footprint.pads.iter().filter(|p| p.kind == PadKind::Tht) {
            let Some(d) = pad.drill else { continue };
            let pos = p.pad_position(fp, pad);
            if d < c.min_drill {
                f.push(finding(
                    "DRC-006",
                    Error,
                    format!(
                        "{} pad {} drill {} below minimum",
                        fp.refdes,
                        pad.number,
                        mm(d as f64)
                    ),
                    vec![fp.id.clone()],
                    Some(pos),
                ));
            }
            let ring = (pad.size_x.min(pad.size_y) - d) / 2;
            if ring < c.min_annular_ring {
                f.push(finding(
                    "DRC-006",
                    Error,
                    format!(
                        "{} pad {} annular ring {} below minimum",
                        fp.refdes,
                        pad.number,
                        mm(ring as f64)
                    ),
                    vec![fp.id.clone()],
                    Some(pos),
                ));
            }
        }
    }

    // Copper items and pairwise clearance (DRC-002) — sweep on x for efficiency.
    let items = copper::items(p);
    let boxes: Vec<Rect> = items.iter().map(|i| i.shape.bbox()).collect();
    let max_req = {
        let mut m = c.min_clearance.max(c.default_class.clearance);
        for nc in &c.net_classes {
            m = m.max(nc.clearance);
        }
        let vmax = c
            .net_voltages_v
            .values()
            .fold(0.0f64, |a, &b| a.max(b.abs()));
        m.max((ipc::clearance_mm(vmax * 2.0, ClearanceCondition::B2) * 1e6) as i64)
    };
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| boxes[i].min.x);
    let mut reported: BTreeSet<(String, String)> = BTreeSet::new();
    for (oi, &i) in order.iter().enumerate() {
        for &j in &order[oi + 1..] {
            if boxes[j].min.x > boxes[i].max.x + max_req {
                break;
            }
            let (a, b) = (&items[i], &items[j]);
            if a.layer != b.layer || a.net == b.net && !a.net.is_empty() {
                continue;
            }
            if a.kind == b.kind && a.object_id == b.object_id {
                continue;
            }
            if !boxes[i].inflate(max_req).intersects(&boxes[j]) {
                continue;
            }
            let mut req = c
                .min_clearance
                .max(c.class_for(&a.net).clearance)
                .max(c.class_for(&b.net).clearance) as f64;
            let mut basis = "net class".to_string();
            if let (Some(va), Some(vb)) =
                (c.net_voltages_v.get(&a.net), c.net_voltages_v.get(&b.net))
            {
                let cond = if a.layer == CopperLayer::Top || a.layer == CopperLayer::Bottom {
                    ClearanceCondition::B2
                } else {
                    ClearanceCondition::B1
                };
                let v_req = ipc::clearance_mm((va - vb).abs(), cond) * 1e6;
                if v_req > req {
                    req = v_req;
                    basis = format!(
                        "IPC-2221B {cond:?} for {:.0} V (unverified table)",
                        (va - vb).abs()
                    );
                }
            }
            let d = distance(&a.shape, &b.shape);
            let key = (a.label(), b.label());
            if d <= 0.0 && !a.net.is_empty() && !b.net.is_empty() {
                if reported.insert(key) {
                    f.push(finding(
                        "DRC-003",
                        Error,
                        format!(
                            "short: {} ({}) touches {} ({}) on {:?}",
                            a.label(),
                            a.net,
                            b.label(),
                            b.net,
                            a.layer
                        ),
                        vec![a.object_id.clone(), b.object_id.clone()],
                        None,
                    ));
                }
            } else if d < req - 1.0 && reported.insert(key) {
                f.push(finding(
                    "DRC-002",
                    Error,
                    format!(
                        "clearance {} between {} ({}) and {} ({}) on {:?} is below {} [{basis}]",
                        mm(d.max(0.0)),
                        a.label(),
                        a.net,
                        b.label(),
                        b.net,
                        a.layer,
                        mm(req)
                    ),
                    vec![a.object_id.clone(), b.object_id.clone()],
                    None,
                ));
            }
        }
    }

    // DRC-005: copper inside board with edge clearance.
    if has_outline {
        let edge = c.board_edge_clearance as f64;
        let mut seen = BTreeSet::new();
        for it in &items {
            let pts: Vec<_> = match &it.shape.core {
                Core::Segment(a, b) => vec![*a, *b],
                Core::Polygon(poly) => poly.clone(),
            };
            let bad = pts.iter().any(|pt| {
                !point_in_polygon(*pt, outline)
                    || (0..outline.len())
                        .map(|k| {
                            point_segment_distance(
                                *pt,
                                outline[k],
                                outline[(k + 1) % outline.len()],
                            )
                        })
                        .fold(f64::INFINITY, f64::min)
                        - it.shape.radius
                        < edge - 1.0
            });
            if bad && seen.insert(it.label()) {
                f.push(finding(
                    "DRC-005",
                    Error,
                    format!(
                        "{} is outside the board or within {} of the edge",
                        it.label(),
                        mm(edge)
                    ),
                    vec![it.object_id.clone()],
                    None,
                ));
            }
        }
        for z in p.pcb.zones.values() {
            if z.outline.iter().any(|q| !point_in_polygon(*q, outline)) {
                f.push(finding("DRC-005", Warning, format!("zone {} extends beyond the board outline (fill is clipped to the board edge clearance)", z.id), vec![z.id.clone()], None));
            }
        }
    }

    // DRC-004: unrouted connections; DRC-013 zone islands.
    let conn = copper::analyse(p);
    for (net, groups) in &conn.net_groups {
        if *groups > 1 {
            let n = conn.ratsnest.iter().filter(|r| &r.0 == net).count();
            f.push(finding(
                "DRC-004",
                Error,
                format!(
                    "net {net} has {n} unrouted connection(s) ({groups} separate copper groups)"
                ),
                vec![],
                conn.ratsnest.iter().find(|r| &r.0 == net).map(|r| r.1),
            ));
        }
    }
    for z in p.pcb.zones.keys() {
        if let Some(fill) = copper::fill_zone(p, z, &items) {
            let mut touched = BTreeSet::new();
            for it in items
                .iter()
                .filter(|i| i.kind == ItemKind::Pad && i.layer == fill.layer && i.net == fill.net)
            {
                touched.extend(fill.islands_touching(&it.shape));
            }
            let isolated = (1..=fill.islands).filter(|k| !touched.contains(k)).count();
            if fill.islands == 0 {
                f.push(finding(
                    "DRC-013",
                    Warning,
                    format!("zone {} has no fill (completely cleared)", z),
                    vec![z.clone()],
                    None,
                ));
            } else if isolated > 0 {
                f.push(finding("DRC-013", Warning, format!("zone {} has {isolated} isolated copper island(s) not connected to any {} pad", z, fill.net), vec![z.clone()], None));
            }
            f.extend(thermal_reliefs(p, &fill));
        }
    }

    // DRC-015: silkscreen over soldermask openings (checks the legend as fabricated).
    for side in [Side::Top, Side::Bottom] {
        f.extend(silk_over_openings(
            &silk::legend(p, side),
            &silk::mask_openings(p, side.copper()),
            silk::MASK_CLEARANCE,
            side,
        ));
    }

    // DRC-007: courtyard overlaps on the same side.
    let fps: Vec<_> = p.pcb.footprints.values().collect();
    let court = |fp: &eak_core::FootprintInstance| {
        p.library.get(&fp.component_id).map(|d| {
            let (w, h) = fp.rotation.apply_size(
                d.footprint.courtyard_half_w * 2,
                d.footprint.courtyard_half_h * 2,
            );
            Rect::centered(fp.position, w, h)
        })
    };
    for i in 0..fps.len() {
        for j in i + 1..fps.len() {
            if fps[i].side != fps[j].side {
                continue;
            }
            if let (Some(a), Some(b)) = (court(fps[i]), court(fps[j])) {
                if a.intersects(&b) {
                    f.push(finding(
                        "DRC-007",
                        Error,
                        format!(
                            "courtyards of {} and {} overlap",
                            fps[i].refdes, fps[j].refdes
                        ),
                        vec![fps[i].id.clone(), fps[j].id.clone()],
                        Some(a.center()),
                    ));
                }
            }
        }
    }

    // DRC-010 / DRC-011: schematic ↔ PCB consistency.
    let nl = netlist::build(p);
    let realised: BTreeSet<&str> = p
        .pcb
        .footprints
        .values()
        .map(|f| f.symbol_id.as_str())
        .collect();
    for s in p.schematic.symbols.values() {
        if !realised.contains(s.id.as_str()) {
            f.push(finding(
                "DRC-011",
                Warning,
                format!(
                    "{} has no footprint on the PCB (run schematic → PCB sync)",
                    s.refdes
                ),
                vec![s.id.clone()],
                None,
            ));
        }
    }
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        let mut expected: BTreeMap<String, String> = BTreeMap::new();
        for (pin, pads) in &def.pin_map {
            if let Some(net) = nl.pin_net.get(&(fp.symbol_id.clone(), pin.clone())) {
                let single = nl
                    .nets
                    .get(net)
                    .is_none_or(|n| n.pins.len() + n.label_ids.len() <= 1);
                if !single {
                    for pad in pads {
                        expected.insert(pad.clone(), net.clone());
                    }
                }
            }
        }
        if expected != fp.pad_nets {
            f.push(finding(
                "DRC-010",
                Error,
                format!(
                    "{} pad nets differ from the schematic netlist (run schematic → PCB sync)",
                    fp.refdes
                ),
                vec![fp.id.clone(), fp.symbol_id.clone()],
                Some(fp.position),
            ));
        }
    }

    // DRC-012: dangling track ends.
    for t in p.pcb.tracks.values() {
        for end in [t.a, t.b] {
            let probe = copper::Shape {
                core: Core::Segment(end, end),
                radius: t.width as f64 / 2.0,
            };
            let touches =
                items.iter().any(|it| {
                    it.layer == t.layer
                        && it.net == t.net
                        && !(it.kind == ItemKind::Track && it.object_id == t.id)
                        && distance(&probe, &it.shape) <= 0.0
                }) || p.pcb.zones.values().any(|z| {
                    z.layer == t.layer && z.net == t.net && point_in_polygon(end, &z.outline)
                });
            if !touches {
                f.push(finding(
                    "DRC-012",
                    Warning,
                    format!("track {} on {} has a dangling end", t.id, t.net),
                    vec![t.id.clone()],
                    Some(end),
                ));
            }
        }
    }

    let rules = vec![
        rule("DRC-001", "Minimum / net-class track width", format!("min {}", mm(c.min_track_width as f64)), "Project constraints"),
        rule("DRC-002", "Copper clearance", format!("min {}, net classes, declared net voltages", mm(c.min_clearance as f64)), "Project constraints; IPC-2221B Table 6-1 B2 for declared voltages (table unverified against standard text)"),
        rule("DRC-003", "Shorts between nets", "", "EAK"),
        rule("DRC-004", "Unrouted connections", "physical copper connectivity incl. zone fill islands (0.1 mm raster)", "EAK"),
        rule("DRC-005", "Board edge clearance", mm(c.board_edge_clearance as f64), "Project constraints"),
        rule("DRC-006", "Drill size / annular ring", format!("drill ≥ {}, ring ≥ {}", mm(c.min_drill as f64), mm(c.min_annular_ring as f64)), "Project constraints"),
        rule("DRC-007", "Courtyard overlap", "", "EAK"),
        rule("DRC-008", "Track current capacity", format!("ΔT {} °C, outer copper {} oz", c.temp_rise_c, p.pcb.board.stackup.outer_copper_oz), "IPC-2221 (via DESK audit; verified_against_standard = false)"),
        rule("DRC-009", "Board outline", "closed simple polygon", "EAK"),
        rule("DRC-010", "Schematic/PCB net consistency", "", "EAK"),
        rule("DRC-011", "Missing footprints", "", "EAK"),
        rule("DRC-012", "Dangling tracks", "", "EAK"),
        rule("DRC-013", "Zone islands", "", "EAK"),
        rule("DRC-014", "Thermal-relief spokes", format!("4 axis spokes per same-net pad; gap = zone clearance; spoke = net-class track width (≥ {}); ≥ {MIN_SPOKES} must land in fill; spoke raster width ≥ min track width; connected spoke width vs declared net current", mm(c.min_track_width as f64)), "EAK relief geometry; current sizing IPC-2221 (via DESK audit; verified_against_standard = false)"),
        rule("DRC-015", "Silkscreen over soldermask openings", format!("legend ink ≥ {} from pad openings (mask expansion {})", mm(silk::MASK_CLEARANCE as f64), mm(p.manufacturing.mask_expansion as f64)), "EAK default (common fabricator practice, not an IPC value; unverified)"),
    ];
    let not_implemented = vec![
        "Controlled impedance and signal integrity".into(),
        "Differential-pair length/skew matching".into(),
        "Thermal analysis and copper current density in zones (only relief-spoke width vs declared net current is checked)".into(),
        "Creepage (surface path) analysis — only voltage-based clearance (air gap) is checked"
            .into(),
        "Silkscreen-to-board-edge, silkscreen overlap/legibility, solder-mask sliver and paste checks".into(),
        "Copper-to-hole and hole-to-hole spacing".into(),
        "Acid traps and copper slivers".into(),
    ];
    let limitations = vec![
        "Zone fill connectivity is evaluated on a 0.1 mm raster; necks narrower than the raster may be misjudged".into(),
        "Thermal reliefs: relief gap and spoke width are derived from the zone clearance and net class (no per-zone/per-pad relief settings in the IR yet); vias and tracks connect solidly".into(),
        "Two copper layers only".into(),
        "Pad shapes: rectangle, circle, oval (no rounded-rectangle or custom polygons)".into(),
    ];
    Report::new("DRC", p, rules, f, not_implemented, limitations)
}

/// Minimum number of relief spokes that must land in zone fill.
pub const MIN_SPOKES: usize = 2;

/// DRC-014 for one zone fill: spoke count, spoke width and current capacity per relief.
fn thermal_reliefs(p: &Project, fill: &copper::ZoneFill) -> Vec<Finding> {
    let c = &p.constraints;
    let mut f = Vec::new();
    for rel in &fill.reliefs {
        let name = p
            .pcb
            .footprints
            .get(&rel.footprint_id)
            .map_or(rel.footprint_id.as_str(), |fp| fp.refdes.as_str());
        let name = format!("{name}.{}", rel.pad);
        let objs = vec![rel.footprint_id.clone(), fill.zone_id.clone()];
        let n = rel.connected_spokes();
        if n < MIN_SPOKES {
            f.push(finding(
                "DRC-014",
                Warning,
                format!(
                    "thermal relief of {name} in zone {} lands {n} of {} spokes in the fill (minimum {MIN_SPOKES})",
                    fill.zone_id,
                    rel.spokes.len()
                ),
                objs.clone(),
                Some(rel.center),
            ));
        }
        for s in rel.spokes.iter().filter(|s| s.connected) {
            if s.width < c.min_track_width {
                f.push(finding(
                    "DRC-014",
                    Error,
                    format!(
                        "thermal spoke {:?} of {name} is {} wide on the fill raster, below the minimum track width {}",
                        s.dir,
                        mm(s.width as f64),
                        mm(c.min_track_width as f64)
                    ),
                    objs.clone(),
                    Some(rel.center),
                ));
            }
        }
        let Some(&amps) = c.net_currents_a.get(&fill.net) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        match ipc::trace_width_mm(
            amps,
            false,
            p.pcb.board.stackup.outer_copper_oz,
            c.temp_rise_c,
        ) {
            Ok(req) => {
                let have = to_mm(rel.connected_width());
                if have + 1e-9 < req {
                    f.push(finding(
                        "DRC-014",
                        // Warning, not error: per-pad current is unknown, and the full net
                        // current is only an upper bound (e.g. every decoupling-cap pad on a
                        // 3 A pour). Widen the zone net class to raise the spoke width.
                        Warning,
                        format!("thermal relief of {name} has {n} spoke(s) totalling {have:.3} mm but {amps:.2} A on {} needs ≥ {req:.3} mm if the full declared net current flows through this pad: verify the pad current or widen the {} net class (IPC-2221 applied to the spoke cross-section; table not verified against standard text)", fill.net, fill.net),
                        objs.clone(),
                        Some(rel.center),
                    ));
                }
            }
            Err(e) => f.push(finding(
                "DRC-014",
                Error,
                format!("current sizing for the thermal relief of {name} failed: {e}"),
                objs.clone(),
                Some(rel.center),
            )),
        }
    }
    f
}

/// DRC-015: legend ink on `side` that overlaps (error) or comes within `clearance`
/// (warning) of a soldermask opening. One finding per offending opening.
pub fn silk_over_openings(
    strokes: &[silk::Stroke],
    openings: &[silk::Opening],
    clearance: Nm,
    side: Side,
) -> Vec<Finding> {
    let mut f = Vec::new();
    for o in openings {
        let ob = o.shape.bbox().inflate(clearance);
        let worst = strokes
            .iter()
            .filter(|s| s.shape().bbox().intersects(&ob))
            .map(|s| distance(&s.shape(), &o.shape))
            .fold(f64::INFINITY, f64::min);
        if worst >= clearance as f64 - 1.0 {
            continue;
        }
        let (severity, what) = if worst <= 0.0 {
            (Error, "overlaps".to_string())
        } else {
            (Warning, format!("is {} from", mm(worst)))
        };
        f.push(finding(
            "DRC-015",
            severity,
            format!(
                "{side:?} silkscreen {what} the soldermask opening of {}.{} (minimum {})",
                o.refdes,
                o.pad,
                mm(clearance as f64)
            ),
            vec![o.footprint_id.clone()],
            None,
        ));
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{FootprintInstance, Point, Rotation, Zone};
    use eak_engineering::copper::pad_shape;

    /// 20 × 20 mm board, one single-pad THT footprint "J1" on GND at (10, 10) and a bottom
    /// GND zone with the given outline.
    fn board(zone: &[Point]) -> Project {
        let mut p = Project::new("drc");
        let mut def = eak_core::builtin::get("CONN_1x02_P2.54").unwrap();
        def.footprint.pads.truncate(1);
        def.footprint.pads[0].position = Point::default();
        p.library.insert(def.id.clone(), def.clone());
        p.pcb.board.outline = vec![
            Point::mm(0.0, 0.0),
            Point::mm(20.0, 0.0),
            Point::mm(20.0, 20.0),
            Point::mm(0.0, 20.0),
        ];
        p.pcb.footprints.insert(
            "fp".into(),
            FootprintInstance {
                id: "fp".into(),
                symbol_id: "sym".into(),
                component_id: def.id.clone(),
                refdes: "J1".into(),
                position: Point::mm(10.0, 10.0),
                rotation: Rotation::R0,
                side: Side::Top,
                pad_nets: [("1".to_string(), "GND".to_string())].into(),
                locked: false,
            },
        );
        p.pcb.zones.insert(
            "z".into(),
            Zone {
                id: "z".into(),
                layer: CopperLayer::Bottom,
                net: "GND".into(),
                outline: zone.to_vec(),
                clearance: 300_000,
            },
        );
        p
    }

    fn drc014(p: &Project) -> Vec<Finding> {
        run(p)
            .findings
            .into_iter()
            .filter(|f| f.rule_id == "DRC-014")
            .collect()
    }

    #[test]
    fn well_connected_relief_passes_and_rules_are_declared() {
        let p = board(&[
            Point::mm(1.0, 1.0),
            Point::mm(19.0, 1.0),
            Point::mm(19.0, 19.0),
            Point::mm(1.0, 19.0),
        ]);
        let r = run(&p);
        assert!(r.findings.iter().all(|f| f.rule_id != "DRC-014"));
        assert!(r.findings.iter().all(|f| f.rule_id != "DRC-015"));
        for id in ["DRC-014", "DRC-015"] {
            assert!(r.rules.iter().any(|x| x.id == id));
        }
        assert!(r.limitations.iter().any(|l| l.contains("Thermal reliefs")));
    }

    #[test]
    fn starved_relief_and_undersized_spokes_are_reported() {
        // Zone strip starting right of the pad edge: only the +X spoke lands.
        let mut p = board(&[
            Point::mm(9.0, 9.6),
            Point::mm(19.0, 9.6),
            Point::mm(19.0, 10.4),
            Point::mm(9.0, 10.4),
        ]);
        let f = drc014(&p);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].severity, Warning);
        assert!(f[0].message.contains("J1.1") && f[0].message.contains("1 of 4"));
        // Declaring 5 A on GND flags the single ~0.3 mm spoke against IPC-2221 (a warning:
        // the per-pad share of the net current is unknown).
        p.constraints.net_currents_a.insert("GND".into(), 5.0);
        let f = drc014(&p);
        assert!(
            f.iter()
                .any(|x| x.severity == Warning && x.message.contains("5.00 A")),
            "{f:?}"
        );
    }

    #[test]
    fn high_current_pour_needs_a_power_class_for_its_reliefs() {
        let mut p = board(&[
            Point::mm(1.0, 1.0),
            Point::mm(19.0, 1.0),
            Point::mm(19.0, 19.0),
            Point::mm(1.0, 19.0),
        ]);
        p.constraints.net_currents_a.insert("GND".into(), 5.0);
        // Default-class spokes (4 x 0.3-0.4 mm raster) are below the ~2.8 mm IPC-2221 width
        // for 5 A.
        let f = drc014(&p);
        assert!(f.iter().any(|x| x.message.contains("5.00 A")), "{f:?}");
        // A power net class widens every spoke and clears the warning.
        let mut power = p.constraints.default_class.clone();
        power.name = "Power".into();
        power.track_width = 1_000_000;
        power.nets = vec!["GND".into()];
        p.constraints.net_classes.push(power);
        assert!(drc014(&p).is_empty(), "{:?}", drc014(&p));
    }

    #[test]
    fn silkscreen_over_or_near_an_opening_is_reported_once_per_opening() {
        let o = silk::Opening {
            footprint_id: "fp".into(),
            refdes: "U1".into(),
            pad: "3".into(),
            shape: pad_shape(
                Point::mm(5.0, 0.0),
                Rotation::R0,
                eak_core::PadShape::Rect,
                1_000_000,
                1_000_000,
            ),
        };
        let line = |y: f64| silk::Stroke {
            a: Point::mm(0.0, y),
            b: Point::mm(10.0, y),
            width: silk::LINE_WIDTH,
        };
        let check = |s: &[silk::Stroke]| {
            silk_over_openings(s, std::slice::from_ref(&o), silk::MASK_CLEARANCE, Side::Top)
        };
        let over = check(&[line(0.0), line(0.2)]);
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].severity, Error);
        assert!(over[0].message.contains("U1.3"));
        // Ink edge 0.05 mm from the opening: closer than the 0.1 mm minimum.
        let near = check(&[line(0.625)]);
        assert_eq!(near.len(), 1);
        assert_eq!(near[0].severity, Warning);
        assert!(check(&[line(0.8)]).is_empty());
        // The clipped legend model is clean against the same opening.
        let clipped = silk::clip(&line(0.0), std::slice::from_ref(&o), silk::MASK_CLEARANCE);
        assert!(!clipped.is_empty());
        assert!(check(&clipped).is_empty());
    }

    #[test]
    fn generated_legend_is_clean_against_its_own_openings() {
        let mut p = board(&[]);
        p.pcb.zones.clear();
        // Move J1's courtyard outline over its own pad by shrinking the courtyard.
        let def = p.library.values_mut().next().unwrap();
        def.footprint.courtyard_half_w = 500_000;
        def.footprint.courtyard_half_h = 500_000;
        let raw = silk::raw_legend(&p, Side::Top);
        let openings = silk::mask_openings(&p, CopperLayer::Top);
        assert!(!silk_over_openings(&raw, &openings, silk::MASK_CLEARANCE, Side::Top).is_empty());
        let r = run(&p);
        assert!(
            r.findings.iter().all(|f| f.rule_id != "DRC-015"),
            "{:?}",
            r.findings
        );
        assert!(!silk::legend(&p, Side::Top).is_empty());
    }
}
