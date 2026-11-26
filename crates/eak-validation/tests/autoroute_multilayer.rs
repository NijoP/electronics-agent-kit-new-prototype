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

/// The board's layout as sorted text rows, object ids left out (they are random per run):
/// footprint placement (refdes, position, rotation, side), tracks (layer, ends, width, net) and
/// vias (position, drill, size, span, net).
fn layout_rows(p: &Project) -> Vec<String> {
    let fps = p.pcb.footprints.values().map(|f| {
        format!(
            "F {} {:?} {:?} {:?}",
            f.refdes, f.position, f.rotation, f.side
        )
    });
    let tracks = p.pcb.tracks.values().map(|t| {
        let (a, b) = if t.a <= t.b { (t.a, t.b) } else { (t.b, t.a) };
        format!("T {:?} {a:?} {b:?} {} {}", t.layer, t.width, t.net)
    });
    let vias = p.pcb.vias.values().map(|v| {
        format!(
            "V {:?} {} {} {:?} {:?} {}",
            v.position, v.drill, v.diameter, v.from, v.to, v.net
        )
    });
    let mut rows: Vec<String> = fps.chain(tracks).chain(vias).collect();
    rows.sort();
    rows
}

/// Route fingerprint: track and via count plus an FNV-1a hash of the layout rows.
fn fingerprint(p: &Project) -> (usize, usize, u64) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in layout_rows(p).join("\n").bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    (p.pcb.tracks.len(), p.pcb.vias.len(), h)
}

/// The same design routes to the same copper on every run and platform (undo/replay and
/// reviewable diffs depend on it). Two builds in one process get different random object ids,
/// which used to reorder the ratsnest and change the routes. On a mismatch the differing rows
/// are printed, so the CI log names what moved.
#[test]
fn the_same_board_routes_to_the_same_copper_every_run() {
    use std::io::Write;
    // Pinned from CI run 45844200699, equal on linux, windows, macos-arm and macos-intel. A
    // deliberate router change updates these values (and says so in its commit).
    let pinned: [(&str, (usize, usize, u64)); 3] = [
        ("route_inner_keepout", (12, 4, 0xe9af_c688_064d_bab4)),
        ("route_ref4", (103, 11, 0x4fae_609d_6a2b_f1a2)),
        ("route_constrained4", (50, 2, 0x9fa3_6954_c61e_e1a7)),
    ];
    for (board, want) in pinned {
        let (e1, _) = build(board, 0);
        let (e2, _) = build(board, 0);
        let (r1, r2) = (layout_rows(e1.project()), layout_rows(e2.project()));
        if r1 != r2 {
            let only = |a: &[String], b: &[String]| -> Vec<String> {
                a.iter()
                    .filter(|r| !b.contains(r))
                    .take(12)
                    .cloned()
                    .collect()
            };
            panic!(
                "{board}: two runs laid out differently\nonly in run 1: {:#?}\nonly in run 2: {:#?}",
                only(&r1, &r2),
                only(&r2, &r1)
            );
        }
        let first = fingerprint(e1.project());
        let _ = writeln!(
            std::io::stderr(),
            "ROUTER-FINGERPRINT {board}: tracks={} vias={} hash={:016x}",
            first.0,
            first.1,
            first.2
        );
        assert_eq!(
            first, want,
            "{board}: route fingerprint changed (see ROUTER-FINGERPRINT)"
        );
    }
}

/// Blind via span (schema v8 `SetViaSpans`): a via keep-out on the bottom layer covers the board,
/// so no through via may be placed (it would reach the bottom), and a top-layer wall stands
/// between the parts. The only way across is In1, reached by the allowed Top-In1 blind span;
/// without that rule SIG stays open.
#[test]
fn a_blind_span_is_used_where_no_through_via_fits() {
    let (e, warnings) = build("route_blind_span", 0);
    eprintln!("{warnings:#?}");
    let p = e.project();
    let sig: Vec<_> = p.pcb.vias.values().filter(|v| v.net == "SIG").collect();
    assert!(sig.len() >= 2, "SIG goes down to In1 and back: {sig:?}");
    for v in &sig {
        assert_eq!(
            (v.from, v.to),
            (CopperLayer::Top, CopperLayer::Inner(1)),
            "{v:?}"
        );
    }
    assert!(p
        .pcb
        .tracks
        .values()
        .any(|t| t.net == "SIG" && t.layer == CopperLayer::Inner(1)));
    evidence("route_blind_span", p, &warnings, Some(2), 2);
    assert_all_connected(p);
    assert_drc_clean(p);

    // Same board without the span rule: through vias only, and none fits for SIG.
    let (mut e, _) = build("route_blind_span", 2);
    let _ = exec(&mut e, json!({"type": "AutoRoute"}));
    let conn = eak_engineering::copper::analyse(e.project());
    assert!(
        conn.net_groups.get("SIG").is_some_and(|g| *g > 1),
        "SIG cannot cross without a blind span"
    );
}

/// Per-net inflation: a 3 A +5V rail (about 1.3 mm wide) inflates the shared grid by more than
/// the 0.67 mm gap between the SOIC-8 pads, so on that grid every pad's own cell is blocked and
/// the escapes found no path (NO_LEGAL_PATH). Each signal net now retries on a grid inflated for
/// its own clearance and width, and the board routes complete and DRC clean.
#[test]
fn fine_pitch_escapes_route_next_to_a_wide_power_net() {
    let (e, warnings) = build("route_fine_pitch", 0);
    eprintln!("{warnings:#?}");
    let p = e.project();
    assert!(
        !warnings.iter().any(|w| w.starts_with("unrouted ")),
        "{warnings:#?}"
    );
    let width = |net: &str| {
        p.pcb
            .tracks
            .values()
            .filter(|t| t.net == net)
            .map(|t| t.width)
            .max()
            .unwrap_or(0)
    };
    assert!(width("+5V") >= 1_000_000, "+5V is current-sized");
    for net in ["RST", "MOSI", "MISO", "SCK"] {
        let w = width(net);
        assert!(w > 0 && w < 500_000, "{net} escapes at its own width: {w}");
    }
    evidence("route_fine_pitch", p, &warnings, None, 0);
    assert_all_connected(p);
    assert_drc_clean(p);
}
