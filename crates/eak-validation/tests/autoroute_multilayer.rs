//! Multilayer autorouter acceptance: boards described as checked-in command scripts
//! (`tests/fixtures/*.json`, typed `Command`s run through `Engine::execute`) are auto-routed,
//! then judged by the engine's own DRC and physical connectivity, never by the router.
//!
//! In a fixture, a string `"@R1"` in `footprint_id` names the footprint with refdes R1 and in any
//! other field the schematic symbol with refdes R1 (ids are generated, refdes are stable).

use eak_core::{CopperLayer, Project};
use eak_runtime::{Actor, Command, CommandRequest, CommandResponse, Engine};
use eak_validation::drc;
use serde_json::{json, Value};

fn resolve(p: &Project, key: &str, v: &mut Value) {
    match v {
        Value::String(s) if s.starts_with('@') => {
            let refdes = &s[1..];
            let id = if key == "footprint_id" {
                p.pcb
                    .footprints
                    .values()
                    .find(|f| f.refdes == refdes)
                    .map(|f| f.id.clone())
            } else {
                p.schematic
                    .symbols
                    .values()
                    .find(|s| s.refdes == refdes)
                    .map(|s| s.id.clone())
            };
            *v = Value::String(
                id.unwrap_or_else(|| panic!("fixture: no {key} with refdes {refdes}")),
            );
        }
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                resolve(p, k, x);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| resolve(p, key, x)),
        _ => {}
    }
}

/// Run one fixture command (JSON, `@refdes` resolved) through the engine.
fn exec(e: &mut Engine, mut v: Value) -> CommandResponse {
    resolve(e.project(), "", &mut v);
    let command: Command =
        serde_json::from_value(v.clone()).unwrap_or_else(|err| panic!("{err}: {v}"));
    e.execute(CommandRequest {
        expected_revision: e.project().revision,
        actor: Actor::human(),
        command,
        approval: None,
    })
}

/// Run the commands of `fixtures/<name>.json` (all but the last `skip`); returns the engine
/// and the warnings of the last command run.
fn build(name: &str, skip: usize) -> (Engine, Vec<String>) {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut script: Vec<Value> = serde_json::from_str(&text).expect("fixture is a JSON array");
    script.truncate(script.len() - skip);
    let mut e = Engine::in_memory(Project::new(name));
    let mut last = vec![];
    for (k, v) in script.into_iter().enumerate() {
        let r = exec(&mut e, v.clone());
        assert!(r.accepted, "step {k} {v} rejected: {:?}", r.error);
        last = r.warnings;
    }
    (e, last)
}

fn assert_drc_clean(p: &Project) {
    let report = drc::run(p);
    for f in &report.findings {
        eprintln!("{} {:?} {}", f.rule_id, f.severity, f.message);
    }
    assert_eq!(report.errors, 0, "DRC errors after autoroute");
}

fn assert_all_connected(p: &Project) {
    let conn = eak_engineering::copper::analyse(p);
    assert!(conn.ratsnest.is_empty(), "unrouted: {:?}", conn.ratsnest);
    assert!(conn.shorts.is_empty(), "shorts: {:?}", conn.shorts);
    for (net, groups) in &conn.net_groups {
        assert_eq!(*groups, 1, "{net} is in {groups} copper groups");
    }
}

/// Asserts the AutoRoute connectivity proof covers every net with pads, each one copper group
/// (`nets` of them when the fixture fixes the count) and at least `min_vias` vias were placed, then
/// prints the evidence line. Written to the process's stderr directly: the test harness captures
/// `println!`/`eprintln!` of passing tests, so CI logs would not show it otherwise.
fn evidence(board: &str, p: &Project, warnings: &[String], nets: Option<usize>, min_vias: usize) {
    use std::io::Write;
    let proof = warnings
        .iter()
        .find(|w| w.starts_with("connectivity proof: all "))
        .unwrap_or_else(|| panic!("{board}: no all-nets proof line in {warnings:#?}"));
    let proven: usize = proof["connectivity proof: all ".len()..]
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{board}: no net count in {proof}"));
    let pad_nets: std::collections::BTreeSet<&String> = p
        .pcb
        .footprints
        .values()
        .flat_map(|f| f.pad_nets.values())
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(
        proven,
        pad_nets.len(),
        "{board}: every net with pads is proven"
    );
    if let Some(n) = nets {
        assert_eq!(proven, n, "{board}: nets proven");
    }
    let vias = p.pcb.vias.len();
    assert!(
        vias >= min_vias,
        "{board}: {vias} vias, expected >= {min_vias}"
    );
    let unrouted = warnings
        .iter()
        .filter(|w| w.starts_with("unrouted "))
        .count();
    let layers = p.pcb.board.stackup.copper_layers();
    let _ = writeln!(
        std::io::stderr(),
        "ROUTER-EVIDENCE {board}: layers={layers} nets_proven={proven} unrouted={unrouted} vias={vias} tracks={} | {proof}",
        p.pcb.tracks.len()
    );
}

#[test]
fn routes_on_the_inner_signal_layer_past_an_outer_keepout_wall() {
    let (e, warnings) = build("route_inner_keepout", 0);
    eprintln!("{warnings:#?}");
    let p = e.project();
    evidence("route_inner_keepout", p, &warnings, Some(2), 2);
    assert!(!p.pcb.tracks.is_empty(), "nothing routed: {warnings:?}");
    let on = |l: CopperLayer| p.pcb.tracks.values().filter(|t| t.layer == l).count();
    assert!(
        on(CopperLayer::Inner(2)) > 0,
        "the wall can only be crossed on In2"
    );
    assert_eq!(
        on(CopperLayer::Inner(1)),
        0,
        "In1 is a plane: never routed on"
    );
    assert!(!p.pcb.vias.is_empty(), "SMD pads reach In2 through vias");
    for v in p.pcb.vias.values() {
        assert_eq!((v.from, v.to), (CopperLayer::Top, CopperLayer::Bottom));
        // Not inside the via keep-out (7.5..11 x 6..14 mm), the DRC also checks DRC-021.
        let (x, y) = (v.position.x as f64 / 1e6, v.position.y as f64 / 1e6);
        assert!(
            !((7.5..=11.0).contains(&x) && (6.0..=14.0).contains(&y)),
            "via at {x},{y}"
        );
    }
    assert_all_connected(p);
    assert_drc_clean(p);
}

#[test]
fn the_proof_is_reported_and_a_closed_path_names_the_keepout_that_closes_it() {
    let (_, warnings) = build("route_inner_keepout", 0);
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("connectivity proof: all ")),
        "{warnings:#?}"
    );

    // Same board, but a wall that keeps tracks out of every layer: no route can exist, and the
    // reason names the region and its rule instead of a bare "no path".
    let (mut e, _) = build("route_inner_keepout", 1);
    let wall = json!({"type": "AddRegion", "name": "full wall", "forbid": ["tracks"], "outline": [
        {"x": 13000000, "y": 0}, {"x": 17000000, "y": 0},
        {"x": 17000000, "y": 20000000}, {"x": 13000000, "y": 20000000}
    ]});
    assert!(exec(&mut e, wall).accepted);
    let r = exec(&mut e, json!({"type": "AutoRoute"}));
    assert!(!r.accepted, "nothing can cross the wall");
    let why = format!("{:?}", r.error);
    assert!(
        why.contains("only keep-out regions close the path"),
        "{why}"
    );
    assert!(
        why.contains("full wall") && why.contains("[DRC-021]"),
        "{why}"
    );
}

/// Acceptance board 1: the EAK reference circuit (5 V header, AMS1117 LDO, ATtiny85, LED, I/O
/// header) on Sig / GND plane / Sig / Sig. Every net, GND included, is auto-routed on the signal
/// layers; the In1 GND pour joins through-hole GND pads by its reliefs.
#[test]
fn four_layer_reference_board_routes_with_every_connection_proven_and_drc_clean() {
    let (e, warnings) = build("route_ref4", 0);
    eprintln!("{warnings:#?}");
    let p = e.project();
    evidence("route_ref4", p, &warnings, None, 0);
    assert_eq!(p.pcb.board.stackup.copper_layers(), 4);
    assert!(
        !warnings.iter().any(|w| w.starts_with("unrouted ")),
        "{warnings:#?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("connectivity proof: all ")),
        "{warnings:#?}"
    );
    assert!(p
        .pcb
        .tracks
        .values()
        .all(|t| t.layer != CopperLayer::Inner(1)));
    assert_all_connected(p);
    assert_drc_clean(p);
}

/// Acceptance board 2: a constrained design on four layers — a fully reversed 4-net bus between
/// two headers, a 1 A +12V rail in a wide net class with declared voltages, a net-pair clearance
/// rule, a board cut-out in the middle, a top-layer keep-out band and a no-via band.
#[test]
fn constrained_four_layer_design_routes_with_every_rule_kept() {
    let (e, warnings) = build("route_constrained4", 0);
    eprintln!("{warnings:#?}");
    let p = e.project();
    evidence("route_constrained4", p, &warnings, Some(6), 0);
    assert!(
        !warnings.iter().any(|w| w.starts_with("unrouted ")),
        "{warnings:#?}"
    );
    for t in p.pcb.tracks.values().filter(|t| t.net == "+12V") {
        assert!(t.width >= 500_000, "+12V keeps its class width: {t:?}");
    }
    for v in p.pcb.vias.values() {
        let x = v.position.x as f64 / 1e6;
        assert!(!(15.0..=19.0).contains(&x), "via in the no-via band: {v:?}");
    }
    assert_all_connected(p);
    assert_drc_clean(p);
}

/// Route fingerprint: track and via count plus an FNV-1a hash of the sorted geometry (layer,
/// end points, width, net; via position, drill, size, span). Object ids are excluded: they are
/// random per run.
fn fingerprint(p: &Project) -> (usize, usize, u64) {
    let mut rows: Vec<String> = p
        .pcb
        .tracks
        .values()
        .map(|t| {
            let (a, b) = if t.a <= t.b {
                (t.a, t.b)
            } else {
                (t.b, t.a)
            };
            format!("T {:?} {a:?} {b:?} {} {}", t.layer, t.width, t.net)
        })
        .chain(p.pcb.vias.values().map(|v| {
            format!(
                "V {:?} {} {} {:?} {:?} {}",
                v.position, v.drill, v.diameter, v.from, v.to, v.net
            )
        }))
        .collect();
    rows.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rows.join("\n").bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    (p.pcb.tracks.len(), p.pcb.vias.len(), h)
}

/// The same design routes to the same copper on every run and platform (undo/replay and
/// reviewable diffs depend on it). Two builds in one process get different random object ids,
/// which used to reorder the ratsnest and change the routes.
#[test]
fn the_same_board_routes_to_the_same_copper_every_run() {
    use std::io::Write;
    for board in ["route_inner_keepout", "route_ref4", "route_constrained4"] {
        let first = fingerprint(build(board, 0).0.project());
        let second = fingerprint(build(board, 0).0.project());
        assert_eq!(first, second, "{board}: two runs routed differently");
        let _ = writeln!(
            std::io::stderr(),
            "ROUTER-FINGERPRINT {board}: tracks={} vias={} hash={:016x}",
            first.0,
            first.1,
            first.2
        );
    }
}
