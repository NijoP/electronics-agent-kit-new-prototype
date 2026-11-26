//! Multilayer autorouter acceptance: boards described as checked-in command scripts
//! (`tests/fixtures/*.json`, typed `Command`s run through `Engine::execute`) are auto-routed,
//! then judged by the engine's own DRC and physical connectivity, never by the router.
//!
//! In a fixture, a string `"@R1"` in `footprint_id` names the footprint with refdes R1 and in any
//! other field the schematic symbol with refdes R1 (ids are generated, refdes are stable).

use eak_core::{CopperLayer, Project};
use eak_runtime::{Actor, Command, CommandRequest, Engine};
use eak_validation::drc;
use serde_json::Value;

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

/// Run every command of `fixtures/<name>.json`; returns the engine and the warnings of the last.
fn build(name: &str) -> (Engine, Vec<String>) {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let script: Vec<Value> = serde_json::from_str(&text).expect("fixture is a JSON array");
    let mut e = Engine::in_memory(Project::new(name));
    let mut last = vec![];
    for (k, mut v) in script.into_iter().enumerate() {
        resolve(e.project(), "", &mut v);
        let command: Command =
            serde_json::from_value(v.clone()).unwrap_or_else(|err| panic!("step {k}: {err}: {v}"));
        let r = e.execute(CommandRequest {
            expected_revision: e.project().revision,
            actor: Actor::human(),
            command,
            approval: None,
        });
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

#[test]
fn routes_on_the_inner_signal_layer_past_an_outer_keepout_wall() {
    let (e, warnings) = build("route_inner_keepout");
    eprintln!("{warnings:#?}");
    let p = e.project();
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
