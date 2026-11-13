//! `build-design`: author the ESP32-C3 dashboard board through the EAK command engine, run
//! EAK's ERC / DRC / MECH checks and compile EAK's manufacturing package.
//!
//! Usage: `cargo run --release -- <output-root>` (default: the `validation/esp32-dashboard`
//! folder this crate lives in). Writes:
//! * `<root>/project/`             native EAK project (project.eak.json + journal.jsonl)
//! * `<root>/outputs/reports/`     ERC/DRC/MECH reports, engineering calcs, netlist, build log
//! * `<root>/outputs/manufacturing/` Gerbers, drill, BOM, CPL, manifest (EAK compiler)

use eak_runtime::Engine;
use eak_validation::{drc, erc, mech, Report};
use esp32_dashboard_design::{build_pcb, build_schematic, report, Driver};
use serde_json::json;
use std::path::{Path, PathBuf};

fn write(path: &Path, data: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn summary(r: &Report) -> String {
    let mut s = format!(
        "{}: revision {} errors {} warnings {} outcome {:?}\n",
        r.check, r.revision, r.errors, r.warnings, r.outcome
    );
    for f in &r.findings {
        s.push_str(&format!("  {} {:?} {}\n", f.rule_id, f.severity, f.message));
    }
    s
}

fn main() {
    let root: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
    let proj_dir = root.join("project");
    let out = root.join("outputs");
    // Regenerate from scratch: only remove what this tool itself produces.
    if proj_dir.join("project.eak.json").exists() {
        std::fs::remove_dir_all(&proj_dir).expect("remove old project");
    }
    if out.join("manufacturing").exists() {
        std::fs::remove_dir_all(out.join("manufacturing")).expect("remove old package");
    }

    let mut engine = Engine::create(&proj_dir, "ESP32-C3 Environment Dashboard").expect("create project");
    let mut d = Driver::new(&mut engine);
    build_schematic(&mut d).unwrap_or_else(|e| panic!("schematic: {e}"));
    let erc_sch = erc::run(d.engine.project());
    let notes = build_pcb(&mut d).unwrap_or_else(|e| panic!("pcb: {e}"));
    let log = std::mem::take(&mut d.log);
    let p = engine.project().clone();

    let erc_r = erc::run(&p);
    let drc_r = drc::run(&p);
    let mech_r = mech::run(&p);
    let rep = out.join("reports");
    write(&rep.join("erc.json"), &serde_json::to_string_pretty(&erc_r).unwrap());
    write(&rep.join("drc.json"), &serde_json::to_string_pretty(&drc_r).unwrap());
    write(&rep.join("mech.json"), &serde_json::to_string_pretty(&mech_r).unwrap());
    write(
        &rep.join("engineering.json"),
        &serde_json::to_string_pretty(&report::compute()).unwrap(),
    );

    // Netlist as resolved by EAK (eak_engineering::netlist) for review and the pin-map test.
    let nl = eak_engineering::netlist::build(&p);
    let nets: serde_json::Map<String, serde_json::Value> = nl
        .nets
        .values()
        .map(|n| {
            (
                n.name.clone(),
                json!(n.pins.iter().map(|x| format!("{}.{} ({})", x.refdes, x.pin, x.pin_name)).collect::<Vec<_>>()),
            )
        })
        .collect();
    write(&rep.join("netlist.json"), &serde_json::to_string_pretty(&nets).unwrap());

    let cmds: Vec<_> = log
        .commands
        .iter()
        .map(|(n, rev, w)| json!({"command": n, "revision": rev, "warnings": w}))
        .collect();
    let routing_notes: Vec<_> = notes.iter().filter(|n| n.starts_with("router") || n.starts_with("unrouted") || n.starts_with("could not")).cloned().collect();
    write(
        &rep.join("build-log.json"),
        &serde_json::to_string_pretty(&json!({
            "project_revision": p.revision,
            "commands_executed": cmds.len(),
            "placement_routing_notes": routing_notes,
            "erc_after_schematic": {"errors": erc_sch.errors, "warnings": erc_sch.warnings},
            "commands": cmds,
        }))
        .unwrap(),
    );

    let m = eak_manufacturing::compile(&p, &[&erc_r, &drc_r, &mech_r], &out.join("manufacturing"))
        .expect("manufacturing compile");
    let problems = eak_manufacturing::verify_package(&out.join("manufacturing")).expect("verify");
    drop(engine); // releases the project lock

    let mut s = String::new();
    s.push_str(&format!("project revision {}\n", p.revision));
    for n in &routing_notes {
        s.push_str(&format!("{n}\n"));
    }
    s.push_str(&summary(&erc_r));
    s.push_str(&summary(&drc_r));
    s.push_str(&summary(&mech_r));
    s.push_str(&format!(
        "manufacturing package: {:?}; blocking: {:?}; files: {}; re-verify problems: {:?}\n",
        m.status,
        m.blocking_reasons,
        m.files.len(),
        problems
    ));
    write(&rep.join("summary.txt"), &s);
    print!("{s}");
}
