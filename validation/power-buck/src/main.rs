//! Build the power-buck board through EAK and write every artefact:
//!
//! ```text
//! <root>/project/        native EAK project folder (project.eak.json, journal, evidence/)
//! <root>/outputs/        EAK manufacturing package (Gerber, Excellon, BOM, CPL, manifest)
//! <root>/reports/        ERC/DRC/MECH reports, engineering calculations, netlist, connectivity
//! ```
//!
//! Usage: `cargo run --release -- [root]` (default root: this package directory).

use eak_power_buck::{calcs, design, verify};
use eak_runtime::Engine;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn write(path: &Path, s: &str) -> Result<(), String> {
    std::fs::write(path, s).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let root: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let proj_dir = root.join("project");
    let out_dir = root.join("outputs");
    let rep_dir = root.join("reports");
    // Regenerate from scratch: the design source is the command sequence in design.rs.
    for d in [&proj_dir, &out_dir, &rep_dir] {
        if d.exists() {
            std::fs::remove_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
    }
    std::fs::create_dir_all(&rep_dir).map_err(|e| e.to_string())?;

    let mut engine = Engine::create(&proj_dir, "power-buck")?;
    let built = design::build(&mut engine)?;
    let (erc_r, drc_r, mech_r) = verify(&engine);
    let mut evidence = vec![];
    for r in [&erc_r, &drc_r, &mech_r] {
        let json = serde_json::to_string_pretty(r).map_err(|e| e.to_string())?;
        write(
            &rep_dir.join(format!("{}.json", r.check.to_lowercase())),
            &json,
        )?;
        if let Some(path) = engine.save_evidence(&r.check, &json) {
            evidence.push(path);
        }
    }
    let p = engine.project().clone();
    let manifest = eak_manufacturing::compile(&p, &[&erc_r, &drc_r, &mech_r], &out_dir)?;
    let reverify = eak_manufacturing::verify_package(&out_dir)?;

    let calc = calcs::run(&p)?;
    write(
        &rep_dir.join("engineering-calcs.json"),
        &serde_json::to_string_pretty(&calc).map_err(|e| e.to_string())?,
    )?;
    write(
        &rep_dir.join("engineering-calcs.md"),
        &calcs::to_markdown(&calc),
    )?;

    // Netlist as resolved by EAK's schematic connectivity engine.
    let nl = eak_engineering::netlist::build(&p);
    let mut net_txt = format!("# EAK netlist, revision {}\n", p.revision);
    for (name, net) in &nl.nets {
        let pins: Vec<String> = net
            .pins
            .iter()
            .map(|x| format!("{}.{}({})", x.refdes, x.pin, x.pin_name))
            .collect();
        let _ = writeln!(net_txt, "{name}: {}", pins.join(" "));
    }
    write(&rep_dir.join("netlist.txt"), &net_txt)?;
    let conn = eak_engineering::copper::analyse(&p);
    write(
        &rep_dir.join("connectivity.json"),
        &serde_json::to_string_pretty(&conn).map_err(|e| e.to_string())?,
    )?;
    write(&rep_dir.join("build-log.txt"), &built.log.join("\n"))?;

    let mut s = String::new();
    let _ = writeln!(s, "project revision : {}", p.revision);
    for r in [&erc_r, &drc_r, &mech_r] {
        let _ = writeln!(
            s,
            "{:<4}             : {:?}, {} errors, {} warnings",
            r.check, r.outcome, r.errors, r.warnings
        );
        for f in &r.findings {
            let _ = writeln!(s, "    {} {:?} {}", f.rule_id, f.severity, f.message);
        }
    }
    let _ = writeln!(s, "package status   : {:?}", manifest.status);
    for b in &manifest.blocking_reasons {
        let _ = writeln!(s, "    blocking: {b}");
    }
    let _ = writeln!(
        s,
        "package files    : {} (re-verification problems: {})",
        manifest.files.len(),
        reverify.len()
    );
    for x in &reverify {
        let _ = writeln!(s, "    {x}");
    }
    // Locate pour islands that touch no pad of their net (DRC-013 reports only a count).
    let items = eak_engineering::copper::items(&p);
    for z in p.pcb.zones.values() {
        let Some(fill) = eak_engineering::copper::fill_zone(&p, &z.id, &items) else {
            continue;
        };
        let mut touched = std::collections::BTreeSet::new();
        for it in items.iter().filter(|i| {
            i.kind == eak_engineering::copper::ItemKind::Pad
                && i.layer == fill.layer
                && i.net == fill.net
        }) {
            touched.extend(fill.islands_touching(&it.shape));
        }
        for k in (1..=fill.islands).filter(|k| !touched.contains(k)) {
            let (mut x0, mut y0, mut x1, mut y1, mut n) =
                (f64::MAX, f64::MAX, f64::MIN, f64::MIN, 0);
            for r in 0..fill.rows {
                for c in 0..fill.cols {
                    if fill.cells[r * fill.cols + c] == k {
                        let pt = fill.cell_center(c, r);
                        let (x, y) = (pt.x as f64 / 1e6, pt.y as f64 / 1e6);
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                        n += 1;
                    }
                }
            }
            let _ = writeln!(
                s,
                "pour island      : {:?} {} island {k} touches no {} pad: {n} cells, x {x0:.1}-{x1:.1} y {y0:.1}-{y1:.1} mm",
                z.layer, z.id, z.net
            );
        }
    }
    let _ = writeln!(s, "net groups       : {:?}", conn.net_groups);
    let _ = writeln!(s, "shorts           : {}", conn.shorts.len());
    let _ = writeln!(s, "evidence files   : {}", evidence.len());
    write(&rep_dir.join("summary.txt"), &s)?;
    print!("{s}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
