//! Phase 4 verification additions: one passing and one failing fixture per rule.
//! ERC-014..016, DRC-018..020 and DFM-001..010 against the reference design.

use eak_core::{PinType, Point, Project, Via};
use eak_engineering::netlist;
use eak_runtime::{fixtures, Engine};
use eak_validation::{dfm, drc, erc, holes, Report, Severity};

fn reference() -> Project {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb(&mut e).expect("pcb");
    e.project().clone()
}

fn fired(r: &Report, rule: &str) -> Vec<String> {
    r.findings
        .iter()
        .filter(|f| f.rule_id == rule)
        .map(|f| format!("{:?} {}", f.severity, f.message))
        .collect()
}

fn declared(r: &Report, rules: &[&str]) {
    for id in rules {
        assert!(r.rules.iter().any(|x| x.id == *id), "{id} not declared");
    }
}

/// A two-pin net whose pins belong to different component definitions and no power port.
fn two_part_net(p: &Project) -> (String, [(String, String); 2]) {
    let nl = netlist::build(p);
    for net in nl.nets.values() {
        if net.pins.len() != 2 || net.has_power_port || erc::is_ground_net(&net.name) {
            continue;
        }
        let comp = |i: usize| &p.schematic.symbols[&net.pins[i].symbol_id].component_id;
        if comp(0) != comp(1) {
            let pin = |i: usize| (comp(i).clone(), net.pins[i].pin.clone());
            return (net.name.clone(), [pin(0), pin(1)]);
        }
    }
    panic!("reference schematic has no two-part net");
}

fn set_pin_type(p: &mut Project, (comp, pin): &(String, String), t: PinType) {
    let def = p.library.get_mut(comp).unwrap();
    def.symbol
        .pins
        .iter_mut()
        .find(|x| &x.number == pin)
        .unwrap()
        .electrical_type = t;
}

#[test]
fn erc_pin_conflict_matrix() {
    let p = reference();
    let r = erc::run(&p);
    declared(&r, &["ERC-014", "ERC-015", "ERC-016"]);
    assert!(
        fired(&r, "ERC-014").is_empty(),
        "{:?}",
        fired(&r, "ERC-014")
    );
    assert!(!r
        .not_implemented
        .iter()
        .any(|s| s.contains("compatibility matrix")));

    let (_, pins) = two_part_net(&p);
    let mut bad = p.clone();
    set_pin_type(&mut bad, &pins[0], PinType::Output);
    set_pin_type(&mut bad, &pins[1], PinType::OpenCollector);
    let f = fired(&erc::run(&bad), "ERC-014");
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].starts_with("Error"), "{f:?}");
    // The matrix is symmetric and leaves compatible pairs alone.
    for (a, b) in [
        (PinType::Output, PinType::OpenCollector),
        (PinType::PowerOut, PinType::OpenCollector),
    ] {
        assert_eq!(erc::pin_conflict(a, b), erc::pin_conflict(b, a));
        assert_eq!(erc::pin_conflict(a, b).unwrap().0, Severity::Error);
    }
    assert!(erc::pin_conflict(PinType::Output, PinType::Input).is_none());
    assert!(erc::pin_conflict(PinType::Passive, PinType::PowerOut).is_none());
}

#[test]
fn erc_undriven_input_net() {
    let p = reference();
    assert!(fired(&erc::run(&p), "ERC-015").is_empty());
    let (net, pins) = two_part_net(&p);
    let mut bad = p.clone();
    set_pin_type(&mut bad, &pins[0], PinType::Input);
    set_pin_type(&mut bad, &pins[1], PinType::Input);
    let f = fired(&erc::run(&bad), "ERC-015");
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].contains(&net), "{f:?}");
}

#[test]
fn erc_power_net_checks() {
    let p = reference();
    let r = erc::run(&p);
    assert!(
        r.findings
            .iter()
            .all(|f| !(f.rule_id == "ERC-016" && f.severity == Severity::Error)),
        "{:?}",
        fired(&r, "ERC-016")
    );
    // A supply output on the ground net is an error.
    let nl = netlist::build(&p);
    let gnd = nl
        .nets
        .values()
        .find(|n| erc::is_ground_net(&n.name) && !n.pins.is_empty())
        .expect("ground net");
    let pin = &gnd.pins[0];
    let mut bad = p.clone();
    let comp = bad.schematic.symbols[&pin.symbol_id].component_id.clone();
    set_pin_type(&mut bad, &(comp, pin.pin.clone()), PinType::PowerOut);
    let f = fired(&erc::run(&bad), "ERC-016");
    assert!(
        f.iter()
            .any(|m| m.starts_with("Error") && m.contains("tied to ground")),
        "{f:?}"
    );
    // A power net without a declared voltage is reported once it loses its declaration.
    let declared_power = p
        .constraints
        .net_voltages_v
        .keys()
        .find(|n| {
            nl.nets.get(*n).is_some_and(|x| {
                x.has_power_port || x.pins.iter().any(|q| q.electrical_type == PinType::PowerIn)
            })
        })
        .cloned();
    if let Some(net) = declared_power {
        assert!(!fired(&r, "ERC-016")
            .iter()
            .any(|m| m.contains(&format!("net {net} has no declared"))));
        let mut undeclared = p.clone();
        undeclared.constraints.net_voltages_v.remove(&net);
        let f = fired(&erc::run(&undeclared), "ERC-016");
        assert!(
            f.iter()
                .any(|m| m.contains(&format!("power net {net} has no declared voltage"))),
            "{f:?}"
        );
    } else {
        // The reference declares no voltages: every power net is reported.
        assert!(!fired(&r, "ERC-016").is_empty());
    }
}

/// A via of another net placed `gap_mm` (copper edge to drill edge) from the first THT hole.
fn via_near_hole(p: &Project, gap_mm: f64) -> Project {
    let h = holes::holes(p)
        .into_iter()
        .find(|h| !h.pad.is_empty())
        .expect("THT hole");
    let mut q = p.clone();
    let d = h.drill / 2 + 300_000 + (gap_mm * 1e6) as i64;
    q.pcb.vias.insert(
        "vx".into(),
        Via {
            id: "vx".into(),
            position: Point::new(h.center.x, h.center.y + d),
            drill: 300_000,
            diameter: 600_000,
            net: "OTHER".into(),
            from: eak_core::CopperLayer::Top,
            to: eak_core::CopperLayer::Bottom,
        },
    );
    q
}

#[test]
fn drc_copper_to_hole_and_hole_to_hole() {
    let p = reference();
    let r = drc::run(&p);
    declared(&r, &["DRC-018", "DRC-019", "DRC-020"]);
    assert!(
        fired(&r, "DRC-018").is_empty(),
        "{:?}",
        fired(&r, "DRC-018")
    );
    // The reference router places same-net vias close together (or stacked): warnings for
    // drill breakout or a duplicate drill, never a spacing error.
    assert!(
        fired(&r, "DRC-019")
            .iter()
            .all(|m| m.starts_with("Warning")),
        "{:?}",
        fired(&r, "DRC-019")
    );
    assert!(!r
        .not_implemented
        .iter()
        .any(|s| s.contains("Copper-to-hole")));
    // Keep-outs are checked since schema v4 (DRC-021, ADR-0042): no longer listed as missing.
    assert!(!r.not_implemented.iter().any(|s| s.contains("Keepout")));
    declared(&r, &["DRC-021"]);

    let bad = drc::run(&via_near_hole(&p, 0.05));
    let f = fired(&bad, "DRC-018");
    assert!(f.iter().any(|m| m.contains("via vx")), "{f:?}");
    let f = fired(&bad, "DRC-019");
    assert!(f.iter().any(|m| m.contains("via vx")), "{f:?}");
    let x = bad
        .findings
        .iter()
        .find(|f| f.rule_id == "DRC-019")
        .unwrap();
    assert_eq!(x.required.as_ref().unwrap().value, 0.5);
}

#[test]
fn drc_silkscreen_to_board_edge() {
    let p = reference();
    let clean = fired(&drc::run(&p), "DRC-020");
    assert!(clean.is_empty(), "{clean:?}");
    let mut bad = p.clone();
    let fp = bad.pcb.footprints.values_mut().next().unwrap();
    fp.position = bad.pcb.board.outline[0];
    let f = fired(&drc::run(&bad), "DRC-020");
    assert!(!f.is_empty() && f[0].starts_with("Warning"), "{f:?}");
}

#[test]
fn dfm_profile_is_sourced_and_reference_board_passes() {
    let fab = dfm::fabricator_standard();
    assert!(fab.source_url.starts_with("https://fabricator.invalid/"));
    assert!(!fab.verified);
    let p = reference();
    let r = dfm::run(&p, &fab);
    assert_eq!(r.check, "DFM");
    assert_eq!(r.errors, 0, "{:?}", r.findings);
    let ids: Vec<String> = (1..=10).map(|i| format!("DFM-{i:03}")).collect();
    declared(&r, &ids.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(r.rules.iter().all(|x| x.source.contains(&fab.source_url)));
    assert!(!r.not_implemented.is_empty());
    assert!(r.limitations.iter().any(|l| l.contains("verified = false")));
}

#[test]
fn dfm_rules_fire_on_failing_fixtures() {
    let p = reference();
    let fab = dfm::fabricator_standard();
    let check = |q: &Project, rule: &str| {
        let f = fired(&dfm::run(q, &fab), rule);
        assert!(!f.is_empty(), "{rule} did not fire");
        assert!(f.iter().any(|m| m.starts_with("Error")), "{f:?}");
    };
    let mut q = p.clone();
    q.pcb.tracks.values_mut().next().expect("track").width = 100_000;
    check(&q, "DFM-001");
    let mut q = p.clone();
    q.constraints.min_clearance = 100_000;
    check(&q, "DFM-002");
    let mut q = via_near_hole(&p, 5.0);
    q.pcb.vias.get_mut("vx").unwrap().drill = 200_000;
    check(&q, "DFM-003");
    let mut q = via_near_hole(&p, 5.0);
    q.pcb.vias.get_mut("vx").unwrap().diameter = 400_000;
    check(&q, "DFM-004");
    check(&via_near_hole(&p, 0.05), "DFM-005");
    let mut q = p.clone();
    q.constraints.board_edge_clearance = 100_000;
    check(&q, "DFM-006");
    let mut q = p.clone();
    for pt in &mut q.pcb.board.outline {
        pt.x *= 20;
    }
    check(&q, "DFM-007");
    let mut q = p.clone();
    q.pcb.board.outline.clear();
    check(&q, "DFM-007");
    let mut few = fab.clone();
    few.max_layers = 1;
    assert!(!fired(&dfm::run(&p, &few), "DFM-008").is_empty());
    let mut q = p.clone();
    q.pcb.board.stackup.board_thickness = 3_200_000;
    check(&q, "DFM-009");
    let mut wide = fab.clone();
    wide.min_silk_line_mm = 0.2;
    assert!(!fired(&dfm::run(&p, &wide), "DFM-010").is_empty());
}

/// Evaluator C1: current and voltage rules evaluate nothing for nets without declarations;
/// the report must say so per net instead of reading as a pass.
#[test]
fn undeclared_current_and_voltage_are_listed_as_not_evaluated() {
    let mut p = reference();
    p.constraints.net_currents_a.clear();
    p.constraints.net_voltages_v.clear();
    let r = drc::run(&p);
    let cur = r
        .limitations
        .iter()
        .find(|l| l.contains("DRC-008") && l.contains("NOT EVALUATED"))
        .expect("current limitation");
    let volt = r
        .limitations
        .iter()
        .find(|l| l.contains("DRC-002") && l.contains("NOT EVALUATED"))
        .expect("voltage limitation");
    assert!(
        cur.contains("GND") && volt.contains("GND"),
        "{cur} / {volt}"
    );
    // Declaring every board net clears both lines.
    for n in drc::undeclared_nets_not_evaluated(&p)
        .iter()
        .flat_map(|l| l.rsplit(": ").next().unwrap().split(", "))
        .map(str::to_string)
        .collect::<Vec<_>>()
    {
        p.constraints.net_currents_a.insert(n.clone(), 0.1);
        p.constraints.net_voltages_v.insert(n, 3.3);
    }
    assert!(drc::undeclared_nets_not_evaluated(&p).is_empty());
    assert!(!drc::run(&p)
        .limitations
        .iter()
        .any(|l| l.contains("NOT EVALUATED")));
}
